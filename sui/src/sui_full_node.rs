// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use sui_storage::IndexStore;
use sui_types::sui_serde::Base64;

use crate::{
    api::{RpcGatewayServer, TransactionBytes},
    rpc_gateway::responses::{ObjectResponse, SuiTypeTag},
};
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use sui_config::{NetworkConfig, PersistedConfig};
use sui_core::{
    authority::ReplicaStore,
    full_node::FullNodeState,
    gateway_types::{
        GetObjectInfoResponse, SuiObjectRef, TransactionEffectsResponse, TransactionResponse,
    },
    sui_json::SuiJsonValue,
};
use sui_core::{
    authority_client::{AuthorityClient, NetworkAuthorityClient},
    full_node::FullNode,
    gateway_state::GatewayTxSeqNumber,
};
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    error::SuiError,
};
use tracing::info;

pub struct SuiFullNode {
    pub client: FullNode,
}

impl SuiFullNode {
    pub async fn start_with_genesis(
        network_config_path: &Path,
        db_path: &Path,
    ) -> anyhow::Result<Self> {
        // Network config is all we need for now
        let network_config: NetworkConfig = PersistedConfig::read(network_config_path)?;

        // Start a full node
        let full_node = make_full_node(db_path.to_path_buf(), &network_config).await?;
        full_node.spawn_tasks().await;
        info!("Started full node ");

        Ok(Self { client: full_node })
    }
}

#[async_trait]
impl RpcGatewayServer for SuiFullNode {
    async fn transfer_coin(
        &self,
        _signer: SuiAddress,
        _object_id: ObjectID,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
        _recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn publish(
        &self,
        _sender: SuiAddress,
        _compiled_modules: Vec<Base64>,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn split_coin(
        &self,
        _signer: SuiAddress,
        _coin_object_id: ObjectID,
        _split_amounts: Vec<u64>,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn merge_coin(
        &self,
        _signer: SuiAddress,
        _primary_coin: ObjectID,
        _coin_to_merge: ObjectID,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn execute_transaction(
        &self,
        _tx_bytes: Base64,
        _signature: Base64,
        _pub_key: Base64,
    ) -> RpcResult<TransactionResponse> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn move_call(
        &self,
        _signer: SuiAddress,
        _package_object_id: ObjectID,
        _module: String,
        _function: String,
        _type_arguments: Vec<SuiTypeTag>,
        _rpc_arguments: Vec<SuiJsonValue>,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn sync_account_state(&self, _address: SuiAddress) -> RpcResult<()> {
        todo!()
    }

    //
    // Read APIs
    //

    async fn get_owned_objects(&self, owner: SuiAddress) -> RpcResult<ObjectResponse> {
        let resp = ObjectResponse {
            objects: self
                .client
                .get_owned_objects(owner)
                .await?
                .iter()
                .map(|w| SuiObjectRef::from(*w))
                .collect(),
        };
        Ok(resp)
    }

    async fn get_object_info(&self, object_id: ObjectID) -> RpcResult<GetObjectInfoResponse> {
        Ok(self
            .client
            .get_object_info(object_id)
            .await?
            .try_into()
            .map_err(|e| anyhow!("{}", e))?)
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.client.state.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.state.get_transactions_in_range(start, end)?)
    }

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.state.get_recent_transactions(count)?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse> {
        Ok(self.client.state.get_transaction(digest).await?)
    }

    async fn get_transactions_by_input_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .client
            .state
            .get_transactions_by_input_object(object)
            .await?)
    }

    async fn get_transactions_by_mutated_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .client
            .state
            .get_transactions_by_mutated_object(object)
            .await?)
    }

    async fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.state.get_transactions_from_addr(addr).await?)
    }

    async fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.state.get_transactions_to_addr(addr).await?)
    }
}

pub async fn make_full_node(
    db_store_path: PathBuf,
    net_config: &NetworkConfig,
) -> Result<FullNode, SuiError> {
    let store = Arc::new(ReplicaStore::open(&db_store_path, None));
    let index_path = db_store_path.join("indexes");
    let indexes = Arc::new(IndexStore::open(index_path, None));

    let val_config = net_config
        .validator_configs()
        .iter()
        .next()
        .expect("Validtor set must be non empty");

    let follower_node_state = FullNodeState::new_with_genesis(
        net_config.committee(),
        store,
        indexes,
        val_config.genesis(),
    )
    .await?;

    let mut authority_clients: BTreeMap<_, AuthorityClient> = BTreeMap::new();
    let mut config = mysten_network::config::Config::new();
    config.connect_timeout = Some(Duration::from_secs(5));
    config.request_timeout = Some(Duration::from_secs(5));
    for validator in net_config
        .validator_configs()
        .iter()
        .next()
        .unwrap()
        .committee_config()
        .validator_set()
    {
        let channel = config.connect_lazy(validator.network_address()).unwrap();
        let client = Arc::new(NetworkAuthorityClient::new(channel));
        authority_clients.insert(validator.public_key(), client);
    }

    Ok(FullNode::new(Arc::new(follower_node_state), authority_clients).unwrap())
}
