use url::Url;

pub struct Item {
    pub scheme: String,
    pub uuid: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub params: Vec<(Box<str>, Box<str>)>,
}

impl From<Item> for Url {
    fn from(value: Item) -> Url {
        let host = value.host.unwrap_or_default();

        let mut url = Url::parse(&format!("{}://{}@{}", value.scheme, value.uuid, host)).unwrap();

        url.set_port(value.port).unwrap();

        {
            let mut pairs = url.query_pairs_mut();

            value.params.iter().for_each(|(k, v)| {
                pairs.append_pair(k, v);
            });
        }

        url
    }
}

#[test]
fn item_into_url() {
    let item = Item {
        scheme: "vless".to_string(),
        uuid: "7d3e6808-d105-4ee4-b904-1f6ed8417f4d".to_string(),
        host: Some("8.218.32.188".to_string()),
        port: Some(3004),
        params: vec![
            ("type".into(), "ws".into()),
            ("host".into(), "speedtest.net".into()),
        ],
    };

    let url: Url = item.into();

    assert_eq!(url.scheme(), "vless");
    assert_eq!(url.username(), "7d3e6808-d105-4ee4-b904-1f6ed8417f4d");
    assert_eq!(url.host_str(), Some("8.218.32.188"));
    assert_eq!(url.port(), Some(3004));

    assert_eq!(
        url.query_pairs()
            .find(|(k, _)| k == "type")
            .map(|(_, v)| v.into_owned()),
        Some("ws".to_string())
    );

    assert_eq!(
        url.query_pairs()
            .find(|(k, _)| k == "host")
            .map(|(_, v)| v.into_owned()),
        Some("speedtest.net".to_string())
    );
}
