// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Faucet, SimpleFaucet};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::debug;

pub use self::faucet_service::{FaucetRequest, FaucetResponse};

mod faucet_service;

#[async_trait]
pub trait FaucetService {
    async fn execute(self, faucet: &(impl Faucet + Send + Sync)) -> FaucetResponse;
}

pub struct Service<F = SimpleFaucet>
where
    F: Faucet + Send + Sync,
{
    inner: Arc<ServiceInner<F>>,
}

impl<F> Clone for Service<F>
where
    F: Faucet + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub struct ServiceInner<F>
where
    F: Faucet + Send + Sync,
{
    faucet: F,
}

impl<F: Faucet + Send + Sync> Service<F> {
    pub fn new(faucet: F) -> Self {
        Self {
            inner: Arc::new(ServiceInner { faucet }),
        }
    }

    pub async fn execute(&self, cmd: FaucetRequest) -> FaucetResponse {
        debug!("Got request: {:?}", cmd);
        let res = dispatch(cmd, &self.inner.faucet).await;
        debug!("Executed response: {:?}", res);
        res
    }
}

pub async fn dispatch(cmd: FaucetRequest, faucet: &(impl Faucet + Send + Sync)) -> FaucetResponse {
    match cmd {
        FaucetRequest::FixedAmountRequest(param) => param.execute(faucet).await,
    }
}

#[cfg(test)]
mod tests {
    use sui_types::base_types::SuiAddress;

    use super::faucet_service::DEFAULT_NUM_COINS;
    use super::*;
    use crate::{setup_network_and_wallet, SimpleFaucet};
    use std::thread;

    #[tokio::test]
    async fn service_should_works() {
        let (network, context, _address) = setup_network_and_wallet().await.unwrap();
        let service = Service::new(SimpleFaucet::new(context).await.unwrap());
        let cloned = service.clone();

        // Try calling the service from a new thread
        let handle = thread::spawn(move || async move {
            let res = cloned
                .execute(FaucetRequest::new_fixed_amount_request(
                    SuiAddress::random_for_testing_only(),
                ))
                .await;
            assert_res_ok(res, DEFAULT_NUM_COINS);
        });
        handle.join().unwrap().await;

        // Try calling the service from the same thread
        let res = service
            .execute(FaucetRequest::new_fixed_amount_request(
                SuiAddress::random_for_testing_only(),
            ))
            .await;
        assert_res_ok(res, DEFAULT_NUM_COINS);
        network.kill().await.unwrap();
    }
}

#[cfg(test)]
pub fn assert_res_ok(res: FaucetResponse, num_objects: usize) {
    assert_eq!(res.error, None);
    assert_eq!(res.transferred_gas_objects.len(), num_objects);
}
