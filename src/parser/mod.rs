mod loader;
pub use loader::*;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::Error;

pub async fn parse<L>(loader: L) -> Result<(), Error>
where
    L: Loader,
{
    let reader = loader.open().await?;
    let mut buffer = BufReader::new(reader).lines();

    while let Some(line) = buffer.next_line().await? {
        println!("{line}")
    }

    Ok(())
}
