use super::SignedPrice;
use futures::prelude::*;
use libp2p::request_response::Codec as RequestResponseCodec;
use std::io;

#[derive(Debug, Clone)]
pub struct SignedPriceExchangeProtocol();

impl AsRef<str> for SignedPriceExchangeProtocol {
    fn as_ref(&self) -> &str {
        "/pragma/oracle/1.0.0"
    }
}

#[derive(Debug, Clone, Default)]
pub struct SignedPriceExchangeCodec();

#[async_trait::async_trait]
impl RequestResponseCodec for SignedPriceExchangeCodec {
    type Protocol = SignedPriceExchangeProtocol;
    type Request = SignedPrice;
    type Response = ();

    async fn read_request<T>(
        &mut self,
        _: &SignedPriceExchangeProtocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        tracing::debug!("Reading SignedPriceRequest...");
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        bcs::from_bytes(&buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        _: &mut T,
    ) -> Result<Self::Response, std::io::Error>
    where
        T: AsyncRead + Unpin + Send,
    {
        Ok(())
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> Result<(), std::io::Error>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data =
            bcs::to_bytes(&req).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        io.write_all(&data).await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        _: &mut T,
        _: Self::Response,
    ) -> Result<(), std::io::Error>
    where
        T: AsyncWrite + Unpin + Send,
    {
        Ok(())
    }
}
