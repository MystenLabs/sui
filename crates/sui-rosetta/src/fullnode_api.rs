// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use futures::StreamExt;
use prometheus::Registry;

use sui_core::authority::AuthorityState;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_json_rpc::api::JsonRpcMetrics;
use sui_json_rpc::governance_api::GovernanceReadApi;
use sui_json_rpc::{get_balance_changes_from_effect, ObjectProviderCache};
use sui_json_rpc_types::{
    Checkpoint, CheckpointId, Coin, DelegatedStake, DryRunTransactionBlockResponse,
    SuiObjectDataOptions, SuiTransactionBlock, SuiTransactionBlockEvents,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_node::SuiNode;
use sui_sdk::{SuiClient, SUI_COIN_TYPE};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest};
use sui_types::crypto::default_hash;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::SuiError;
use sui_types::gas_coin::GAS;
use sui_types::quorum_driver_types::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
};
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::{TransactionData, VerifiedTransaction};

use crate::errors::Error;

#[async_trait]
pub trait FullNodeApi: Send + Sync + Clone + 'static {
    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Error>;

    async fn get_checkpoint(&self, id: CheckpointId) -> Result<Checkpoint, Error>;

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<SuiTransactionBlockResponse, Error>;

    async fn get_stakes(&self, owner: SuiAddress) -> Result<Vec<DelegatedStake>, Error>;

    async fn get_sui(&self, owner: SuiAddress) -> Result<Vec<Coin>, Error>;

    async fn select_coins(&self, address: SuiAddress, amount: u128) -> Result<Vec<Coin>, Error>;

    async fn get_reference_gas_price(&self) -> Result<u64, Error>;

    async fn get_object_refs(&self, object_ids: Vec<ObjectID>) -> Result<Vec<ObjectRef>, Error>;

    async fn get_latest_sui_system_state(&self) -> Result<SuiSystemStateSummary, Error>;

    async fn execute_transaction_block(
        &self,
        tx: VerifiedTransaction,
    ) -> Result<SuiTransactionBlockResponse, Error>;

    async fn dry_run_transaction_block(
        &self,
        tx: TransactionData,
    ) -> Result<DryRunTransactionBlockResponse, Error>;
}

#[derive(Clone)]
pub struct RemoteFullNode {
    client: SuiClient,
}

impl RemoteFullNode {
    pub fn new(sui_client: SuiClient) -> Self {
        Self { client: sui_client }
    }
}

#[async_trait]
impl FullNodeApi for RemoteFullNode {
    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Error> {
        Ok(self
            .client
            .read_api()
            .get_latest_checkpoint_sequence_number()
            .await?)
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> Result<Checkpoint, Error> {
        Ok(self.client.read_api().get_checkpoint(id).await?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<SuiTransactionBlockResponse, Error> {
        Ok(self
            .client
            .read_api()
            .get_transaction_with_options(
                digest,
                SuiTransactionBlockResponseOptions::new()
                    .with_input()
                    .with_effects()
                    .with_balance_changes()
                    .with_events(),
            )
            .await?)
    }

    async fn get_stakes(&self, owner: SuiAddress) -> Result<Vec<DelegatedStake>, Error> {
        Ok(self.client.governance_api().get_stakes(owner).await?)
    }

    async fn get_sui(&self, owner: SuiAddress) -> Result<Vec<Coin>, Error> {
        Ok(self
            .client
            .coin_read_api()
            .get_coins_stream(owner, Some(SUI_COIN_TYPE.to_string()))
            .collect::<Vec<_>>()
            .await)
    }

    async fn select_coins(&self, address: SuiAddress, amount: u128) -> Result<Vec<Coin>, Error> {
        Ok(self
            .client
            .coin_read_api()
            .select_coins(address, None, amount, vec![])
            .await?)
    }

    async fn get_reference_gas_price(&self) -> Result<u64, Error> {
        Ok(self.client.read_api().get_reference_gas_price().await?)
    }

    async fn get_object_refs(&self, object_ids: Vec<ObjectID>) -> Result<Vec<ObjectRef>, Error> {
        Ok(self
            .client
            .read_api()
            .multi_get_object_with_options(object_ids, SuiObjectDataOptions::default())
            .await?
            .into_iter()
            .map(|stake| stake.into_object().map(|o| o.object_ref()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(SuiError::from)?)
    }

    async fn get_latest_sui_system_state(&self) -> Result<SuiSystemStateSummary, Error> {
        Ok(self
            .client
            .governance_api()
            .get_latest_sui_system_state()
            .await?)
    }

    async fn execute_transaction_block(
        &self,
        tx: VerifiedTransaction,
    ) -> Result<SuiTransactionBlockResponse, Error> {
        Ok(self
            .client
            .quorum_driver_api()
            .execute_transaction_block(
                tx,
                SuiTransactionBlockResponseOptions::new().with_effects(),
                None,
            )
            .await?)
    }

    async fn dry_run_transaction_block(
        &self,
        tx: TransactionData,
    ) -> Result<DryRunTransactionBlockResponse, Error> {
        Ok(self.client.read_api().dry_run_transaction_block(tx).await?)
    }
}

#[derive(Clone)]
pub struct LocalFullNode {
    state: Arc<AuthorityState>,
    transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
    governance_api: GovernanceReadApi,
}

impl LocalFullNode {
    pub fn new(sui_node: &SuiNode, registry: &Registry) -> Self {
        Self {
            state: sui_node.state(),
            transaction_orchestrator: sui_node
                .transaction_orchestrator()
                .expect("Transaction orchestrator not initialized"),
            governance_api: GovernanceReadApi::new(
                sui_node.state(),
                Arc::new(JsonRpcMetrics::new(registry)),
            ),
        }
    }
}

#[async_trait]
impl FullNodeApi for LocalFullNode {
    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Error> {
        Ok(self.state.get_latest_checkpoint_sequence_number()?)
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> Result<Checkpoint, Error> {
        let cp = match id {
            CheckpointId::SequenceNumber(seq) => {
                self.state.get_verified_checkpoint_by_sequence_number(seq)?
            }
            CheckpointId::Digest(digest) => self
                .state
                .get_verified_checkpoint_summary_by_digest(digest)?,
        };

        let sigs = cp.auth_sig().signature.clone();
        let summary = cp.into_inner().into_data();
        let content = self.state.get_checkpoint_contents(summary.content_digest)?;
        Ok((summary, content, sigs).into())
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<SuiTransactionBlockResponse, Error> {
        // Fetch transaction to determine existence
        let transaction = self.state.get_transaction_block(digest).await?;
        let input_objects = transaction
            .data()
            .inner()
            .intent_message
            .value
            .input_objects()
            .unwrap_or_default();
        // Fetch effects when `show_events` is true because events relies on effects
        let effects = self.state.get_executed_effects(digest)?;
        let object_cache = ObjectProviderCache::new(self.state.clone());
        let balance_changes =
            get_balance_changes_from_effect(&object_cache, &effects, input_objects, None).await?;
        let events = if let Some(event_digest) = effects.events_digest() {
            let events = self.state.get_transaction_events(event_digest)?;
            SuiTransactionBlockEvents::try_from(
                events,
                digest,
                None,
                // threading the epoch_store through this API does not
                // seem possible, so we just read it from the state and fetch
                // the module cache out of it.
                // Notice that no matter what module cache we get things
                // should work
                self.state
                    .load_epoch_store_one_call_per_task()
                    .module_cache()
                    .as_ref(),
            )?
        } else {
            // events field will be Some if and only if `show_events` is true and
            // there is no error in converting fetching events
            SuiTransactionBlockEvents::default()
        };

        let transaction = SuiTransactionBlock::try_from(
            transaction.into_message(),
            self.state
                .load_epoch_store_one_call_per_task()
                .module_cache(),
        )?;

        let mut response = SuiTransactionBlockResponse::new(digest);
        response.balance_changes = Some(balance_changes);
        response.transaction = Some(transaction);
        response.events = Some(events);
        response.effects = Some(effects.try_into()?);
        Ok(response)
    }

    async fn get_stakes(&self, owner: SuiAddress) -> Result<Vec<DelegatedStake>, Error> {
        Ok(self
            .governance_api
            .get_stakes(owner)
            .await
            .map_err(|e| anyhow!(e))?)
    }

    async fn get_sui(&self, owner: SuiAddress) -> Result<Vec<Coin>, Error> {
        Ok(self
            .state
            .indexes
            .as_ref()
            .expect("indexing service not initialized")
            .get_owned_coins_iterator_with_cursor(
                owner,
                (GAS::type_().to_string(), ObjectID::ZERO),
                usize::MAX,
                true,
            )?
            .map(|(coin_type, coin_object_id, coin)| Coin {
                coin_type,
                coin_object_id,
                version: coin.version,
                digest: coin.digest,
                balance: coin.balance,
                previous_transaction: coin.previous_transaction,
            })
            .collect::<Vec<_>>())
    }

    async fn select_coins(&self, address: SuiAddress, amount: u128) -> Result<Vec<Coin>, Error> {
        let mut total = 0u128;
        let coins = self
            .state
            .indexes
            .as_ref()
            .expect("indexing service not initialized")
            .get_owned_coins_iterator_with_cursor(
                address,
                (GAS::type_().to_string(), ObjectID::ZERO),
                usize::MAX,
                true,
            )?
            .take_while(|(_, _, coin)| {
                let ready = total < amount;
                total += coin.balance as u128;
                ready
            })
            .map(|(coin_type, coin_object_id, coin)| Coin {
                coin_type,
                coin_object_id,
                version: coin.version,
                digest: coin.digest,
                balance: coin.balance,
                previous_transaction: coin.previous_transaction,
            })
            .collect::<Vec<_>>();
        if total < amount {
            return Err(Error::InsufficientFund { address, amount });
        }
        return Ok(coins);
    }

    async fn get_reference_gas_price(&self) -> Result<u64, Error> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        Ok(epoch_store.reference_gas_price())
    }

    async fn get_object_refs(&self, object_ids: Vec<ObjectID>) -> Result<Vec<ObjectRef>, Error> {
        self.state
            .get_objects(&object_ids)
            .await?
            .into_iter()
            .zip(object_ids.into_iter())
            .map(|(o, id)| {
                o.map(|o| o.compute_object_reference())
                    .ok_or_else(|| Error::DataError(format!("Object [{id}] is missing.")))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    async fn get_latest_sui_system_state(&self) -> Result<SuiSystemStateSummary, Error> {
        Ok(self
            .state
            .database
            .get_sui_system_state_object()?
            .into_sui_system_state_summary())
    }

    async fn execute_transaction_block(
        &self,
        tx: VerifiedTransaction,
    ) -> Result<SuiTransactionBlockResponse, Error> {
        let response = self
            .transaction_orchestrator
            .execute_transaction_block(ExecuteTransactionRequest {
                transaction: tx.into_inner(),
                request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
            })
            .await
            .map_err(|e| anyhow!(e))?;

        let ExecuteTransactionResponse::EffectsCert(cert) = response;
        let (effects, _, is_executed_locally) = *cert;

        Ok(SuiTransactionBlockResponse {
            digest: *effects.effects.transaction_digest(),
            effects: Some(effects.effects.try_into()?),
            confirmed_local_execution: Some(is_executed_locally),
            ..Default::default()
        })
    }

    async fn dry_run_transaction_block(
        &self,
        tx: TransactionData,
    ) -> Result<DryRunTransactionBlockResponse, Error> {
        let txn_digest = TransactionDigest::new(default_hash(&tx));
        Ok(self.state.dry_exec_transaction(tx, txn_digest).await?.0)
    }
}
