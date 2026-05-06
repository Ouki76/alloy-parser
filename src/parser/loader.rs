use crate::Error;

use std::path::PathBuf;

use futures_util::StreamExt;
use reqwest::Client;
use tokio::{fs::File, io::AsyncRead};
use tokio_util::io::StreamReader;
use url::Url;

pub trait Loader {
    fn open(&self)
    -> impl Future<Output = Result<Box<dyn AsyncRead + Unpin + Send>, Error>> + Send;
}

pub struct FileLoader {
    pub path: PathBuf,
}

impl Loader for FileLoader {
    async fn open(&self) -> Result<Box<dyn AsyncRead + Unpin + Send>, Error> {
        let file = File::open(&self.path).await?;

        Ok(Box::new(file))
    }
}

pub struct RemoteLoader {
    pub url: Url,
    pub client: Client,
}

impl Loader for RemoteLoader {
    async fn open(&self) -> Result<Box<dyn AsyncRead + Unpin + Send>, Error> {
        let resp = self.client.get(self.url.as_str()).send().await?;
        let stream = resp
            .bytes_stream()
            .map(|r| r.map_err(std::io::Error::other));

        Ok(Box::new(StreamReader::new(stream)))
    }
}
