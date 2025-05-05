/*
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};

/// An endpoint for RPC calls.
pub struct Server<I: AsyncRead, O: AsyncWrite> {
    input: BufReader<I>,
    output: BufWriter<O>,
}

#[derive(Error, Debug)]
pub enum Error {}

impl<I: AsyncRead, O: AsyncWrite> Server<I, O> {
    fn new(input: I, output: O) -> Self {
        Self {
            input: BufReader::new(input),
            output: BufWriter::new(output),
        }
    }

    async fn batch_receive<R: DeserializeOwned>(&mut self) -> Result<R, anyhow::Error> {}
}
*/
