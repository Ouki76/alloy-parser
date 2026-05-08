use std::{path::PathBuf, time::Duration};

use alloy_parser::{self, Error};
use clap::Parser;
use futures_util::{StreamExt, stream};
use tokio::{net::TcpStream, time::timeout};

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    output_file: Option<String>,

    #[arg(short, long)]
    input: String,

    #[arg(short, long)]
    geoip_database: Option<PathBuf>,

    #[arg(short, long)]
    check_available: bool,

    #[arg(short, long)]
    auto_naming: bool,
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
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Cli::parse();

    if let Err(e) = args.validate() {
        e.exit();
    }

    let items = if args.input.starts_with("http") {
        let client = reqwest::Client::builder()
            .user_agent(format!(
                "{}/{}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_NAME")
            ))
            .build()?;

        let loader = alloy_parser::RemoteLoader {
            url: args
                .input
                .parse()
                .map_err(|e: url::ParseError| Error::other(e.to_string()))?,
            client,
        };

        alloy_parser::parse(loader).await?
    } else {
        let loader = alloy_parser::FileLoader {
            path: args.input.into(),
        };

        alloy_parser::parse(loader).await?
    };

    let items = if args.check_available {
        let stream = stream::iter(items.into_iter())
            .map(|item| async move {
                let duration = Duration::from_secs(3);

                if let Some(ref host) = item.host {
                    match timeout(duration, TcpStream::connect(&host)).await {
                        Ok(_) => Some(item),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .buffer_unordered(10);

        stream.filter_map(|r| async move { r }).collect().await
    } else {
        items
    };

    if args.auto_naming {}

    println!(
        "{:?}",
        items
            .into_iter()
            .map(|i| i.into())
            .map(|i: url::Url| i.to_string())
            .collect::<Vec<String>>()
    );

    Ok(())
}
