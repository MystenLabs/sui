// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastcrypto::encoding::Base64;
use futures::stream;
use futures::StreamExt;
use futures_core::Stream;
use jsonrpsee::core::client::Subscription;

use crate::error::{Error, SuiRpcResult};
use crate::{RpcClient, WAIT_FOR_TX_TIMEOUT_SEC};
use sui_json_rpc::api::GovernanceReadApiClient;
use sui_json_rpc::api::{
    CoinReadApiClient, IndexerApiClient, MoveUtilsClient, ReadApiClient, WriteApiClient,
};
use sui_json_rpc_types::{
    Balance, Checkpoint, CheckpointId, Coin, CoinPage, DelegatedStake,
    DryRunTransactionBlockResponse, DynamicFieldPage, EventFilter, EventPage, ObjectsPage,
    SuiCoinMetadata, SuiCommittee, SuiEvent, SuiGetPastObjectRequest, SuiMoveNormalizedModule,
    SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery, SuiPastObjectResponse,
    SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
    SuiTransactionBlockResponseQuery, TransactionBlocksPage,
};
use sui_json_rpc_types::{CheckpointPage, SuiLoadedChildObjectsResponse};
use sui_types::balance::Supply;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::error::TRANSACTION_NOT_FOUND_MSG_PREFIX;
use sui_types::event::EventID;
use sui_types::messages::{ExecuteTransactionRequestType, TransactionData, VerifiedTransaction};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::sui_serde::BigInt;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;

#[derive(Debug)]
pub struct ReadApi {
    api: Arc<RpcClient>,
}

impl ReadApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }

    pub async fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> SuiRpcResult<ObjectsPage> {
        Ok(self
            .api
            .http
            .get_owned_objects(address, query, cursor, limit)
            .await?)
    }

    pub async fn get_dynamic_fields(
        &self,
        object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> SuiRpcResult<DynamicFieldPage> {
        Ok(self
            .api
            .http
            .get_dynamic_fields(object_id, cursor, limit)
            .await?)
    }

    pub async fn try_get_parsed_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: SuiObjectDataOptions,
    ) -> SuiRpcResult<SuiPastObjectResponse> {
        Ok(self
            .api
            .http
            .try_get_past_object(object_id, version, Some(options))
            .await?)
    }

    pub async fn try_multi_get_parsed_past_object(
        &self,
        past_objects: Vec<SuiGetPastObjectRequest>,
        options: SuiObjectDataOptions,
    ) -> SuiRpcResult<Vec<SuiPastObjectResponse>> {
        Ok(self
            .api
            .http
            .try_multi_get_past_objects(past_objects, Some(options))
            .await?)
    }

    pub async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: SuiObjectDataOptions,
    ) -> SuiRpcResult<SuiObjectResponse> {
        Ok(self.api.http.get_object(object_id, Some(options)).await?)
    }

    pub async fn multi_get_object_with_options(
        &self,
        object_ids: Vec<ObjectID>,
        options: SuiObjectDataOptions,
    ) -> SuiRpcResult<Vec<SuiObjectResponse>> {
        Ok(self
            .api
            .http
            .multi_get_objects(object_ids, Some(options))
            .await?)
    }

    pub async fn get_total_transaction_blocks(&self) -> SuiRpcResult<u64> {
        Ok(*self.api.http.get_total_transaction_blocks().await?)
    }

    pub async fn get_transaction_with_options(
        &self,
        digest: TransactionDigest,
        options: SuiTransactionBlockResponseOptions,
    ) -> SuiRpcResult<SuiTransactionBlockResponse> {
        Ok(self
            .api
            .http
            .get_transaction_block(digest, Some(options))
            .await?)
    }

    pub async fn multi_get_transactions_with_options(
        &self,
        digests: Vec<TransactionDigest>,
        options: SuiTransactionBlockResponseOptions,
    ) -> SuiRpcResult<Vec<SuiTransactionBlockResponse>> {
        Ok(self
            .api
            .http
            .multi_get_transaction_blocks(digests, Some(options))
            .await?)
    }

    pub async fn get_committee_info(
        &self,
        epoch: Option<BigInt<u64>>,
    ) -> SuiRpcResult<SuiCommittee> {
        Ok(self.api.http.get_committee_info(epoch).await?)
    }

    pub async fn query_transaction_blocks(
        &self,
        query: SuiTransactionBlockResponseQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> SuiRpcResult<TransactionBlocksPage> {
        Ok(self
            .api
            .http
            .query_transaction_blocks(query, cursor, limit, Some(descending_order))
            .await?)
    }

    /// Return a checkpoint
    pub async fn get_checkpoint(&self, id: CheckpointId) -> SuiRpcResult<Checkpoint> {
        Ok(self.api.http.get_checkpoint(id).await?)
    }

    /// Return paginated list of checkpoints
    pub async fn get_checkpoints(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> SuiRpcResult<CheckpointPage> {
        Ok(self
            .api
            .http
            .get_checkpoints(cursor, limit, descending_order)
            .await?)
    }

    /// Return the sequence number of the latest checkpoint that has been executed
    pub async fn get_latest_checkpoint_sequence_number(
        &self,
    ) -> SuiRpcResult<CheckpointSequenceNumber> {
        Ok(*self
            .api
            .http
            .get_latest_checkpoint_sequence_number()
            .await?)
    }

    pub fn get_transactions_stream(
        &self,
        query: SuiTransactionBlockResponseQuery,
        cursor: Option<TransactionDigest>,
        descending_order: bool,
    ) -> impl Stream<Item = SuiTransactionBlockResponse> + '_ {
        stream::unfold(
            (vec![], cursor, true, query),
            move |(mut data, cursor, first, query)| async move {
                if let Some(item) = data.pop() {
                    Some((item, (data, cursor, false, query)))
                } else if (cursor.is_none() && first) || cursor.is_some() {
                    let page = self
                        .query_transaction_blocks(
                            query.clone(),
                            cursor,
                            Some(100),
                            descending_order,
                        )
                        .await
                        .ok()?;
                    let mut data = page.data;
                    data.reverse();
                    data.pop()
                        .map(|item| (item, (data, page.next_cursor, false, query)))
                } else {
                    None
                }
            },
        )
    }

    pub async fn get_normalized_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> SuiRpcResult<BTreeMap<String, SuiMoveNormalizedModule>> {
        Ok(self
            .api
            .http
            .get_normalized_move_modules_by_package(package)
            .await?)
    }

    // TODO(devx): we can probably cache this given an epoch
    pub async fn get_reference_gas_price(&self) -> SuiRpcResult<u64> {
        Ok(*self.api.http.get_reference_gas_price().await?)
    }

    pub async fn dry_run_transaction_block(
        &self,
        tx: TransactionData,
    ) -> SuiRpcResult<DryRunTransactionBlockResponse> {
        Ok(self
            .api
            .http
            .dry_run_transaction_block(Base64::from_bytes(&bcs::to_bytes(&tx)?))
            .await?)
    }

    pub async fn get_loaded_child_objects(
        &self,
        digest: TransactionDigest,
    ) -> SuiRpcResult<SuiLoadedChildObjectsResponse> {
        Ok(self.api.http.get_loaded_child_objects(digest).await?)
    }
}

#[derive(Debug, Clone)]
pub struct CoinReadApi {
    api: Arc<RpcClient>,
}

impl CoinReadApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }

    pub async fn get_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> SuiRpcResult<CoinPage> {
        Ok(self
            .api
            .http
            .get_coins(owner, coin_type, cursor, limit)
            .await?)
    }

    pub async fn get_all_coins(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> SuiRpcResult<CoinPage> {
        Ok(self.api.http.get_all_coins(owner, cursor, limit).await?)
    }

    pub fn get_coins_stream(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> impl Stream<Item = Coin> + '_ {
        stream::unfold(
            (
                vec![],
                /* cursor */ None,
                /* has_next_page */ true,
                coin_type,
            ),
            move |(mut data, cursor, has_next_page, coin_type)| async move {
                if let Some(item) = data.pop() {
                    Some((item, (data, cursor, /* has_next_page */ true, coin_type)))
                } else if has_next_page {
                    let page = self
                        .get_coins(owner, coin_type.clone(), cursor, Some(100))
                        .await
                        .ok()?;
                    let mut data = page.data;
                    data.reverse();
                    data.pop().map(|item| {
                        (
                            item,
                            (data, page.next_cursor, page.has_next_page, coin_type),
                        )
                    })
                } else {
                    None
                }
            },
        )
    }

    pub async fn select_coins(
        &self,
        address: SuiAddress,
        coin_type: Option<String>,
        amount: u128,
        exclude: Vec<ObjectID>,
    ) -> SuiRpcResult<Vec<Coin>> {
        let mut total = 0u128;
        let coins = self
            .get_coins_stream(address, coin_type)
            .filter(|coin: &Coin| future::ready(!exclude.contains(&coin.coin_object_id)))
            .take_while(|coin: &Coin| {
                let ready = future::ready(total < amount);
                total += coin.balance as u128;
                ready
            })
            .collect::<Vec<_>>()
            .await;

        if total < amount {
            return Err(Error::InsufficientFund { address, amount });
        }
        Ok(coins)
    }

    pub async fn get_balance(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> SuiRpcResult<Balance> {
        Ok(self.api.http.get_balance(owner, coin_type).await?)
    }

    pub async fn get_all_balances(&self, owner: SuiAddress) -> SuiRpcResult<Vec<Balance>> {
        Ok(self.api.http.get_all_balances(owner).await?)
    }

    pub async fn get_coin_metadata(
        &self,
        coin_type: String,
    ) -> SuiRpcResult<Option<SuiCoinMetadata>> {
        Ok(self.api.http.get_coin_metadata(coin_type).await?)
    }

    pub async fn get_total_supply(&self, coin_type: String) -> SuiRpcResult<Supply> {
        Ok(self.api.http.get_total_supply(coin_type).await?)
    }
}

#[derive(Clone)]
pub struct EventApi {
    api: Arc<RpcClient>,
}

impl EventApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }

    pub async fn subscribe_event(
        &self,
        filter: EventFilter,
    ) -> SuiRpcResult<impl Stream<Item = SuiRpcResult<SuiEvent>>> {
        match &self.api.ws {
            Some(c) => {
                let subscription: Subscription<SuiEvent> = c.subscribe_event(filter).await?;
                Ok(subscription.map(|item| Ok(item?)))
            }
            _ => Err(Error::Subscription(
                "Subscription only supported by WebSocket client.".to_string(),
            )),
        }
    }

    pub async fn get_events(&self, digest: TransactionDigest) -> SuiRpcResult<Vec<SuiEvent>> {
        Ok(self.api.http.get_events(digest).await?)
    }

    pub async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> SuiRpcResult<EventPage> {
        Ok(self
            .api
            .http
            .query_events(query, cursor, limit, Some(descending_order))
            .await?)
    }

    pub fn get_events_stream(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
        descending_order: bool,
    ) -> impl Stream<Item = SuiEvent> + '_ {
        stream::unfold(
            (vec![], cursor, true, query),
            move |(mut data, cursor, first, query)| async move {
                if let Some(item) = data.pop() {
                    Some((item, (data, cursor, false, query)))
                } else if (cursor.is_none() && first) || cursor.is_some() {
                    let page = self
                        .query_events(query.clone(), cursor, Some(100), descending_order)
                        .await
                        .ok()?;
                    let mut data = page.data;
                    data.reverse();
                    data.pop()
                        .map(|item| (item, (data, page.next_cursor, false, query)))
                } else {
                    None
                }
            },
        )
    }
}

#[derive(Clone)]
pub struct QuorumDriverApi {
    api: Arc<RpcClient>,
}

impl QuorumDriverApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }

    /// Execute a transaction with a FullNode client. `request_type`
    /// defaults to `ExecuteTransactionRequestType::WaitForLocalExecution`.
    /// When `ExecuteTransactionRequestType::WaitForLocalExecution` is used,
    /// but returned `confirmed_local_execution` is false, the client polls
    /// the fullnode until the fullnode recognizes this transaction, or
    /// until times out (see WAIT_FOR_TX_TIMEOUT_SEC). If it times out, an
    /// error is returned from this call.
    pub async fn execute_transaction_block(
        &self,
        tx: VerifiedTransaction,
        options: SuiTransactionBlockResponseOptions,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> SuiRpcResult<SuiTransactionBlockResponse> {
        let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();
        let request_type = request_type.unwrap_or_else(|| options.default_execution_request_type());
        let mut response: SuiTransactionBlockResponse = self
            .api
            .http
            .execute_transaction_block(
                tx_bytes,
                signatures,
                Some(options),
                Some(request_type.clone()),
            )
            .await?;

        Ok(match request_type {
            ExecuteTransactionRequestType::WaitForEffectsCert => response,
            ExecuteTransactionRequestType::WaitForLocalExecution => {
                if let Some(confirmed_local_execution) = response.confirmed_local_execution {
                    if !confirmed_local_execution {
                        Self::wait_until_fullnode_sees_tx(
                            &self.api,
                            *response
                                .effects
                                .as_ref()
                                .map(|e| e.transaction_digest())
                                .ok_or_else(|| {
                                    Error::DataError("Expect effects to be non-empty".to_string())
                                })?,
                        )
                        .await?;
                    }
                }
                response.confirmed_local_execution = Some(true);
                response
            }
        })
    }

    async fn wait_until_fullnode_sees_tx(
        c: &RpcClient,
        tx_digest: TransactionDigest,
    ) -> SuiRpcResult<()> {
        let start = Instant::now();
        loop {
            let resp = ReadApiClient::get_transaction_block(
                &c.http,
                tx_digest,
                Some(SuiTransactionBlockResponseOptions::new()),
            )
            .await;
            if let Err(err) = resp {
                if err.to_string().contains(TRANSACTION_NOT_FOUND_MSG_PREFIX) {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                } else {
                    // immediately return on other types of errors
                    return Err(Error::TransactionConfirmationError(tx_digest, err));
                }
            } else {
                return Ok(());
            }
            if start.elapsed().as_secs() >= WAIT_FOR_TX_TIMEOUT_SEC {
                return Err(Error::FailToConfirmTransactionStatus(
                    tx_digest,
                    WAIT_FOR_TX_TIMEOUT_SEC,
                ));
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct GovernanceApi {
    api: Arc<RpcClient>,
}

impl GovernanceApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }

    /// Return all [DelegatedStake].
    pub async fn get_stakes(&self, owner: SuiAddress) -> SuiRpcResult<Vec<DelegatedStake>> {
        Ok(self.api.http.get_stakes(owner).await?)
    }

    /// Return the committee information for the asked `epoch`.
    /// `epoch`: The epoch of interest. If None, default to the latest epoch
    pub async fn get_committee_info(
        &self,
        epoch: Option<BigInt<u64>>,
    ) -> SuiRpcResult<SuiCommittee> {
        Ok(self.api.http.get_committee_info(epoch).await?)
    }

    /// Return the latest SUI system state object on-chain.
    pub async fn get_latest_sui_system_state(&self) -> SuiRpcResult<SuiSystemStateSummary> {
        Ok(self.api.http.get_latest_sui_system_state().await?)
    }

    /// Return the reference gas price for the network
    pub async fn get_reference_gas_price(&self) -> SuiRpcResult<u64> {
        Ok(*self.api.http.get_reference_gas_price().await?)
    }
}
