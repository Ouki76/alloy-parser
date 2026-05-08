use std::{future::Future, net::IpAddr, path::PathBuf, sync::Arc, time::Duration};

use alloy_parser::{self, Error, Item};
use clap::Parser;
use futures_util::{StreamExt, stream};
use maxminddb::{Reader, geoip2};
use tokio::{
    net::{TcpStream, lookup_host},
    task,
    time::timeout,
};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[arg(short, long)]
    output_file: Option<String>,

    #[arg(short, long)]
    input: String,

    #[arg(short, long)]
    check_available: bool,

    #[arg(long, default_value_t = 3)]
    timeout_secs: u64,

    #[arg(short, long)]
    auto_naming: bool,

    #[arg(short, long)]
    geoip_database: Option<PathBuf>,
}

impl Cli {
    fn validate(&self) -> Result<(), clap::Error> {
        if self.geoip_database.is_some() && !self.auto_naming {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::ArgumentConflict,
                "geoip database can only be used with auto naming",
            ));
        }

        Ok(())
    }

    fn naming_mode(&self) -> NamingMode {
        match (&self.auto_naming, &self.geoip_database) {
            (false, _) => NamingMode::None,
            (true, None) => NamingMode::Index,
            (true, Some(path)) => NamingMode::GeoIp(path.clone()),
        }
    }
}

enum NamingMode {
    None,
    Index,
    GeoIp(PathBuf),
}

#[derive(Clone)]
struct GeoIp {
    reader: Arc<Reader<Vec<u8>>>,
}

impl GeoIp {
    fn new(path: PathBuf) -> Result<Self, Error> {
        let reader = Reader::open_readfile(path).map_err(|e| Error::other(e.to_string()))?;

        Ok(Self {
            reader: Arc::new(reader),
        })
    }

    async fn country(&self, ip: IpAddr) -> Option<String> {
        let reader = self.reader.clone();

        task::spawn_blocking(move || {
            let result = reader.lookup(ip).ok()?;
            let decoded = result.decode::<geoip2::Country>().ok()??;

            decoded.country.names.english.map(str::to_string)
        })
        .await
        .ok()?
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Cli::parse();
    args.validate().map_err(|e| Error::other(e.to_string()))?;

    let items = load_items(&args).await?;
    let items = filter_available(items, &args).await;
    let items = apply_names(items, &args).await?;

    print_items(items);

    Ok(())
}

async fn load_items(args: &Cli) -> Result<Vec<Item>, Error> {
    if is_remote(&args.input) {
        load_remote(&args.input).await
    } else {
        load_local(&args.input).await
    }
}

fn is_remote(input: &str) -> bool {
    input.starts_with("http://") || input.starts_with("https://")
}

async fn load_remote(input: &str) -> Result<Vec<Item>, Error> {
    let client = reqwest::Client::builder()
        .user_agent(format!(
            "{}/{}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;

    let loader = alloy_parser::RemoteLoader {
        url: input
            .parse()
            .map_err(|e: url::ParseError| Error::other(e.to_string()))?,
        client,
    };

    alloy_parser::parse(loader).await
}

async fn load_local(input: &str) -> Result<Vec<Item>, Error> {
    alloy_parser::parse(alloy_parser::FileLoader { path: input.into() }).await
}

async fn filter_available(items: Vec<Item>, args: &Cli) -> Vec<Item> {
    if !args.check_available {
        return items;
    }

    let timeout_duration = Duration::from_secs(args.timeout_secs);

    concurrent_filter_map(items, 30, move |item| async move {
        if item.is_available(timeout_duration).await {
            Some(item)
        } else {
            None
        }
    })
    .await
}

async fn apply_names(items: Vec<Item>, args: &Cli) -> Result<Vec<Item>, Error> {
    match args.naming_mode() {
        NamingMode::None => Ok(items),

        NamingMode::Index => Ok(apply_index_names(items)),

        NamingMode::GeoIp(path) => apply_geo_names(items, path).await,
    }
}

fn apply_index_names(items: Vec<Item>) -> Vec<Item> {
    items
        .into_iter()
        .enumerate()
        .map(|(i, mut item)| {
            item.fragment = Some(i.to_string());
            item
        })
        .collect()
}

async fn apply_geo_names(items: Vec<Item>, path: PathBuf) -> Result<Vec<Item>, Error> {
    let geo = GeoIp::new(path)?;

    let items = concurrent_filter_map(items, 20, move |mut item| {
        let geo = geo.clone();

        async move {
            let ip = item.resolve_ip().await?;
            let country = geo.country(ip).await?;

            item.fragment = Some(country);

            Some(item)
        }
    })
    .await;

    Ok(items)
}

fn print_items(items: Vec<Item>) {
    let urls = items
        .into_iter()
        .map(|item| -> url::Url { item.into() })
        .map(|url| url.to_string())
        .collect::<Vec<_>>();

    println!("{:#?}", urls);
}

async fn concurrent_filter_map<I, O, F, Fut>(items: Vec<I>, concurrency: usize, f: F) -> Vec<O>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Option<O>> + Send,
{
    stream::iter(items)
        .map(f)
        .buffer_unordered(concurrency)
        .filter_map(|x| async move { x })
        .collect()
        .await
}
trait ItemExt {
    fn host_and_port(&self) -> Option<(&str, u16)>;
}

impl ItemExt for Item {
    fn host_and_port(&self) -> Option<(&str, u16)> {
        Some((self.host.as_deref()?, self.port?))
    }
}

trait ItemAsyncExt {
    fn resolve_ip(&self) -> impl Future<Output = Option<IpAddr>> + Send;

    fn is_available(&self, timeout_duration: Duration) -> impl Future<Output = bool> + Send;
}

impl ItemAsyncExt for Item {
    async fn resolve_ip(&self) -> Option<IpAddr> {
        let (host, port) = self.host_and_port()?;

        resolve_ip(host, port).await
    }

    async fn is_available(&self, timeout_duration: Duration) -> bool {
        let Some(ip) = self.resolve_ip().await else {
            return false;
        };

        let Some((_, port)) = self.host_and_port() else {
            return false;
        };

        matches!(
            timeout(timeout_duration, TcpStream::connect((ip, port))).await,
            Ok(Ok(_))
        )
    }
}

async fn resolve_ip(host: &str, port: u16) -> Option<IpAddr> {
    if let Ok(ip) = host.parse() {
        return Some(ip);
    }

    lookup_host((host, port))
        .await
        .ok()?
        .next()
        .map(|addr| addr.ip())
}
