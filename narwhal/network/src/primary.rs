use crate::{CancelHandler2 as CancelHandler, RetryConfig};
use crypto::traits::VerifyingKey;
use futures::FutureExt;
use std::{collections::HashMap, net::SocketAddr};
use tonic::transport::Channel;
use types::{BincodeEncodedPayload, PrimaryMessage, PrimaryToPrimaryClient};

pub struct PrimaryNetwork {
    clients: HashMap<SocketAddr, PrimaryToPrimaryClient<Channel>>,
    retry_config: RetryConfig,
}

impl Default for PrimaryNetwork {
    fn default() -> Self {
        let retry_config = RetryConfig {
            // Retry for forever
            retrying_max_elapsed_time: None,
            ..Default::default()
        };

        Self {
            clients: Default::default(),
            retry_config,
        }
    }
}

impl PrimaryNetwork {
    fn client(&mut self, address: SocketAddr) -> PrimaryToPrimaryClient<Channel> {
        self.clients
            .entry(address)
            .or_insert_with(|| Self::create_client(address))
            .clone()
    }

    fn create_client(address: SocketAddr) -> PrimaryToPrimaryClient<Channel> {
        // TODO use TLS
        let url = format!("http://{}", address);
        let channel = Channel::from_shared(url)
            .expect("URI should be valid")
            .connect_lazy();
        PrimaryToPrimaryClient::new(channel)
    }

    pub async fn send<T: VerifyingKey>(
        &mut self,
        address: SocketAddr,
        message: &PrimaryMessage<T>,
    ) -> CancelHandler<()> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        self.send_message(address, message).await
    }

    async fn send_message(
        &mut self,
        address: SocketAddr,
        message: BincodeEncodedPayload,
    ) -> CancelHandler<()> {
        let client = self.client(address);
        let handle = tokio::spawn(
            self.retry_config
                .retry(move || {
                    let mut client = client.clone();
                    let message = message.clone();
                    async move { client.send_message(message).await.map_err(Into::into) }
                })
                .map(|response| {
                    response.expect("we retry forever so this shouldn't fail");
                }),
        );

        CancelHandler(handle)
    }

    pub async fn broadcast<T: VerifyingKey>(
        &mut self,
        addresses: Vec<SocketAddr>,
        message: &PrimaryMessage<T>,
    ) -> Vec<CancelHandler<()>> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut handlers = Vec::new();
        for address in addresses {
            let handle = self.send_message(address, message.clone()).await;
            handlers.push(handle);
        }
        handlers
    }
}
