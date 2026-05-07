mod loader;
pub use loader::*;

use crate::Error;

use tokio::io::{AsyncBufReadExt, BufReader};
use url::Url;

pub struct Item {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub params: Vec<(Box<str>, Box<str>)>,
}

pub async fn parse<L>(loader: L) -> Result<Vec<Item>, Error>
where
    L: Loader,
{
    let reader = loader.open().await?;
    let mut buffer = BufReader::new(reader).lines();

    let mut items = Vec::new();

    // Getting stream lines
    while let Some(line) = buffer.next_line().await? {
        // If is url
        if let Ok(url) = Url::parse(&line) {
            // Create item
            let item = Item {
                host: url.host().map(|h| h.to_string()),
                port: url.port(),
                params: url
                    .query_pairs()
                    .map(|(k, v)| {
                        (
                            k.into_owned().into_boxed_str(),
                            v.into_owned().into_boxed_str(),
                        )
                    })
                    .collect(),
            };

            // Push item in vec
            items.push(item);
        }
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    use httpmock::{Method::GET, MockServer};
    use tempfile::NamedTempFile;

    fn content() -> String {
        String::from(
            r#"
vless://7d3e6808-d105-4ee4-b904-1f6ed8417f4d@8.218.32.188:3004?type=ws&host=speedtest.net#0025
vless://11111111-d105-4ee4-b904-1f6ed8417f4d@1.1.1.1:443?security=tls&type=grpc#cloudflare
invalid-url
vless://22222222-d105-4ee4-b904-1f6ed8417f4d@google.com:8443?path=/ws&host=cdn.google.com
            "#,
        )
    }

    fn create_file_loader() -> (NamedTempFile, FileLoader) {
        // Create temp file
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();

        // Write content in temp file
        std::fs::write(&path, content()).unwrap();

        // Return file loader
        (temp_file, FileLoader { path })
    }

    fn create_remote_loader() -> (MockServer, RemoteLoader) {
        // Create mock server
        let server = MockServer::start();

        // Create mock items.txt with content
        server.mock(|when, then| {
            when.method(GET).path("/items.txt");
            then.status(200).body(content());
        });

        // Create url obj
        let items_url = server.url("/items.txt").parse::<Url>().unwrap();

        // Create reqwest client
        let client = reqwest::Client::new();

        // Return remote loader
        (
            server,
            RemoteLoader {
                url: items_url,
                client,
            },
        )
    }

    async fn assert_items<L>(loader: L)
    where
        L: Loader,
    {
        let items = parse(loader).await.unwrap();

        // Count
        assert_eq!(items.len(), 3);

        // Hosts
        assert_eq!(items[0].host.as_deref(), Some("8.218.32.188"));
        assert_eq!(items[1].host.as_deref(), Some("1.1.1.1"));
        assert_eq!(items[2].host.as_deref(), Some("google.com"));

        // Ports
        assert_eq!(items[0].port, Some(3004));
        assert_eq!(items[1].port, Some(443));
        assert_eq!(items[2].port, Some(8443));

        // Params
        assert_eq!(
            items[0]
                .params
                .iter()
                .find(|(k, _)| k.as_ref() == "type")
                .map(|(_, v)| v.as_ref()),
            Some("ws")
        );

        assert_eq!(
            items[1]
                .params
                .iter()
                .find(|(k, _)| k.as_ref() == "security")
                .map(|(_, v)| v.as_ref()),
            Some("tls")
        );

        assert_eq!(
            items[2]
                .params
                .iter()
                .find(|(k, _)| k.as_ref() == "path")
                .map(|(_, v)| v.as_ref()),
            Some("/ws")
        );
    }

    #[tokio::test]
    async fn parse_file_loader() {
        let (_file, loader) = create_file_loader();

        assert_items(loader).await;
    }

    #[tokio::test]
    async fn parse_remote_loader() {
        let (_server, loader) = create_remote_loader();

        assert_items(loader).await;
    }
}
