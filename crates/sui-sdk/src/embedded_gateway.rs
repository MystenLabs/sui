// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
// TODO: Remove this file when sim test supports jsonrpc/ws
use std::sync::Arc;

use async_trait::async_trait;

use sui_config::gateway::GatewayConfig;
use sui_config::{NetworkConfig, PersistedConfig, SUI_NETWORK_CONFIG};
use sui_core::gateway_state::{GatewayClient, GatewayState, TxSeqNumber};
use sui_json_rpc_types::{
    EventPage, GetObjectDataResponse, GetRawObjectDataResponse, SuiObjectInfo,
    SuiTransactionResponse,
};
use sui_transaction_builder::{DataReader, TransactionBuilder};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::event::EventID;
use sui_types::messages::{ExecuteTransactionRequestType, VerifiedTransaction};
use sui_types::query::EventQuery;

use crate::TransactionExecutionResult;

#[derive(Clone)]
pub struct SuiClient {
    transaction_builder: TransactionBuilder,
    read_api: Arc<ReadApi>,
    event_api: EventApi,
    quorum_driver: QuorumDriver,
    wallet_sync_api: WalletSyncApi,
}

impl SuiClient {
    pub fn transaction_builder(&self) -> &TransactionBuilder {
        &self.transaction_builder
    }
    pub fn read_api(&self) -> &ReadApi {
        &self.read_api
    }
    pub fn quorum_driver(&self) -> &QuorumDriver {
        &self.quorum_driver
    }
    pub fn event_api(&self) -> &EventApi {
        &self.event_api
    }
    pub fn wallet_sync_api(&self) -> &WalletSyncApi {
        &self.wallet_sync_api
    }
}

impl SuiClient {
    pub async fn new(sui_config_dir: &Path) -> Result<Self, anyhow::Error> {
        let network_path = sui_config_dir.join(SUI_NETWORK_CONFIG);
        let network_conf: NetworkConfig = PersistedConfig::read(&network_path)?;
        let db_folder_path = sui_config_dir.join("gateway_client_db");
        let gateway_conf = GatewayConfig {
            validator_set: network_conf.validator_set().to_owned(),
            db_folder_path,
            ..Default::default()
        };
        let api = GatewayState::create_client(&gateway_conf, None).await?;
        let read_api = Arc::new(ReadApi { api: api.clone() });
        let quorum_driver = QuorumDriver { api: api.clone() };
        let transaction_builder =
            TransactionBuilder(Arc::new(GatewayClientDataReader::new(api.clone())));
        let wallet_sync_api = WalletSyncApi(api);

        Ok(Self {
            transaction_builder,
            read_api,
            event_api: EventApi,
            quorum_driver,
            wallet_sync_api,
        })
    }

    pub fn available_rpc_methods(&self) -> Vec<String> {
        vec![]
    }

    pub fn available_subscriptions(&self) -> Vec<String> {
        vec![]
    }

    pub fn api_version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    pub fn check_api_version(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

pub struct ReadApi {
    api: GatewayClient,
}

impl ReadApi {
    pub async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        Ok(self.api.get_objects_owned_by_address(address).await?)
    }

    pub async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        Ok(self.api.get_objects_owned_by_object(object_id).await?)
    }

    pub async fn get_parsed_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetObjectDataResponse> {
        Ok(self.api.get_object(object_id).await?)
    }

    pub async fn get_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetRawObjectDataResponse> {
        Ok(self.api.get_raw_object(object_id).await?)
    }

    pub async fn get_total_transaction_number(&self) -> anyhow::Result<u64> {
        Ok(self.api.get_total_transaction_number()?)
    }

    pub async fn get_transactions_in_range(
        &self,
        start: TxSeqNumber,
        end: TxSeqNumber,
    ) -> anyhow::Result<Vec<TransactionDigest>> {
        Ok(self.api.get_transactions_in_range(start, end)?)
    }

    pub async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> anyhow::Result<SuiTransactionResponse> {
        Ok(self.api.get_transaction(digest).await?)
    }
}

#[derive(Clone)]
pub struct QuorumDriver {
    api: GatewayClient,
}

impl QuorumDriver {
    pub async fn execute_transaction(
        &self,
        tx: VerifiedTransaction,
        _request_type: Option<ExecuteTransactionRequestType>,
    ) -> anyhow::Result<TransactionExecutionResult> {
        let resp = self.api.execute_transaction(tx.into_inner()).await?;
        Ok(TransactionExecutionResult {
            tx_digest: resp.certificate.transaction_digest,
            tx_cert: Some(resp.certificate),
            effects: Some(resp.effects),
            confirmed_local_execution: true,
            timestamp_ms: resp.timestamp_ms,
            parsed_data: resp.parsed_data,
        })
    }
}

#[derive(Clone)]
pub struct WalletSyncApi(GatewayClient);

impl WalletSyncApi {
    pub async fn sync_account_state(&self, address: SuiAddress) -> anyhow::Result<()> {
        self.0.sync_account_state(address).await?;
        Ok(())
    }
}

pub struct GatewayClientDataReader(GatewayClient);

impl GatewayClientDataReader {
    pub fn new(state: GatewayClient) -> Self {
        Self(state)
    }
}

#[async_trait]
impl DataReader for GatewayClientDataReader {
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<SuiObjectInfo>, anyhow::Error> {
        self.0.get_objects_owned_by_address(address).await
    }

    async fn get_object(
        &self,
        object_id: ObjectID,
    ) -> Result<GetRawObjectDataResponse, anyhow::Error> {
        self.0.get_raw_object(object_id).await
    }
}

#[derive(Clone)]
pub struct EventApi;

impl EventApi {
    pub async fn get_events(
        &self,
        _query: EventQuery,
        _cursor: Option<EventID>,
        _limit: Option<usize>,
        _descending_order: Option<bool>,
    ) -> anyhow::Result<EventPage> {
        panic!("Event not supported by gateway")
    }
}
