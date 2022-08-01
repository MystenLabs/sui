// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use sui_core::{
    authority_aggregator::AuthorityAggregator, authority_client::NetworkAuthorityClient,
};
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    crypto::EmptySignInfo,
    messages::{TransactionEffects, TransactionEnvelope},
    object::{Object, ObjectRead, Owner},
};

use futures::FutureExt;
use sui_quorum_driver::QuorumDriverHandler;
use sui_types::{
    base_types::SuiAddress,
    crypto::AccountKeyPair,
    messages::{
        ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
        Transaction,
    },
};
use test_utils::messages::make_transfer_sui_transaction;
use tracing::log::error;

// This is the maximum gas we will transfer from primary coin into any gas coin
// for running the benchmark
pub const MAX_GAS_FOR_TESTING: u64 = 1_000_000_000;

pub type Gas = (ObjectRef, Owner);

pub type UpdatedAndNewlyMinted = (ObjectRef, ObjectRef);

pub async fn transfer_sui_for_testing(
    gas: Gas,
    keypair: &AccountKeyPair,
    value: u64,
    address: SuiAddress,
    client: &AuthorityAggregator<NetworkAuthorityClient>,
) -> Option<UpdatedAndNewlyMinted> {
    let tx = make_transfer_sui_transaction(
        gas.0,
        address,
        Some(value),
        gas.1.get_owner_address().unwrap(),
        keypair,
    );
    let quorum_driver_handler = QuorumDriverHandler::new(client.clone());
    let qd = quorum_driver_handler.clone_quorum_driver();
    qd.execute_transaction(ExecuteTransactionRequest {
        transaction: tx.clone(),
        request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
    })
    .map(move |res| match res {
        Ok(ExecuteTransactionResponse::EffectsCert(result)) => {
            let (_, effects) = *result;
            let minted = effects.effects.created.get(0).unwrap().0;
            let updated = effects
                .effects
                .mutated
                .iter()
                .find(|(k, _)| k.0 == gas.0 .0)
                .unwrap()
                .0;
            Some((updated, minted))
        }
        Ok(resp) => {
            error!("Unexpected response while transferring sui: {:?}", resp);
            None
        }
        Err(err) => {
            error!("Error while transferring sui: {:?}", err);
            None
        }
    })
    .await
}

pub async fn get_latest(
    object_id: ObjectID,
    aggregator: &AuthorityAggregator<NetworkAuthorityClient>,
) -> Option<Object> {
    // Return the latest object version
    match aggregator.get_object_info_execute(object_id).await.unwrap() {
        ObjectRead::Exists(_, object, _) => Some(object),
        _ => None,
    }
}

pub async fn submit_transaction(
    transaction: Transaction,
    aggregator: &AuthorityAggregator<NetworkAuthorityClient>,
) -> Option<TransactionEffects> {
    let qd = QuorumDriverHandler::new(aggregator.clone());
    if let ExecuteTransactionResponse::EffectsCert(result) = qd
        .clone_quorum_driver()
        .execute_transaction(ExecuteTransactionRequest {
            transaction,
            request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap()
    {
        let (_, effects) = *result;
        Some(effects.effects)
    } else {
        None
    }
}

pub trait Payload: Send + Sync {
    fn make_new_payload(
        self: Box<Self>,
        new_object: ObjectRef,
        new_gas: ObjectRef,
    ) -> Box<dyn Payload>;
    fn make_transaction(&self) -> TransactionEnvelope<EmptySignInfo>;
    fn get_object_id(&self) -> ObjectID;
    fn get_workload_type(&self) -> WorkloadType;
}

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub enum WorkloadType {
    SharedCounter,
    TransferObject,
}

#[async_trait]
pub trait Workload<T: Payload + ?Sized>: Send + Sync {
    async fn init(&mut self, aggregator: &AuthorityAggregator<NetworkAuthorityClient>);
    async fn make_test_payloads(
        &self,
        count: u64,
        client: &AuthorityAggregator<NetworkAuthorityClient>,
    ) -> Vec<Box<T>>;
}
