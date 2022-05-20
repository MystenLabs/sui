// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};
use sui_storage::IndexStore;
use sui_types::{crypto::get_key_pair, sui_serde::Base64};

use crate::{
    api::{RpcGatewayServer, TransactionBytes},
    rpc_gateway::responses::{ObjectResponse, SuiTypeTag},
};
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use sui_config::{NetworkConfig, PersistedConfig};
use sui_core::{
    authority::{AuthorityState, AuthorityStore},
    authority_active::{gossip::gossip_process, ActiveAuthority},
    gateway_types::{
        GetObjectInfoResponse, SuiObjectRef, TransactionEffectsResponse, TransactionResponse,
    },
    sui_json::SuiJsonValue,
};
use sui_core::{authority_client::NetworkAuthorityClient, gateway_state::GatewayTxSeqNumber};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use tracing::info;

pub struct SuiFullNode {
    pub state: Arc<AuthorityState>,
}

impl SuiFullNode {
    pub async fn start_with_genesis(
        network_config_path: &Path,
        db_path: &Path,
    ) -> anyhow::Result<Self> {
        // Network config is all we need for now
        let config: NetworkConfig = PersistedConfig::read(network_config_path)?;

        let (_addr, key_pair) = get_key_pair();

        // TODO use ReplicaStore, or (more likely) get rid of ReplicaStore and
        // use run-time configuration to determine how much state AuthorityStore
        // keeps
        let store = Arc::new(AuthorityStore::open(&db_path, None));

        let index_path = db_path.join("indexes");
        let indexes = Arc::new(IndexStore::open(index_path, None));

        let val_config = config
            .validator_configs()
            .iter()
            .next()
            .expect("Validtor set must be non empty");

        let state = Arc::new(
            AuthorityState::new(
                config.committee().clone(),
                *key_pair.public_key_bytes(),
                Arc::pin(key_pair.copy()),
                store,
                Some(indexes),
                None,
                val_config.genesis(),
            )
            .await,
        );

        let mut net_config = mysten_network::config::Config::new();
        net_config.connect_timeout = Some(Duration::from_secs(5));
        net_config.request_timeout = Some(Duration::from_secs(5));

        let mut authority_clients = BTreeMap::new();
        for validator in config
            .validator_configs()
            .iter()
            .next()
            .unwrap()
            .committee_config()
            .validator_set()
        {
            let channel = net_config
                .connect_lazy(validator.network_address())
                .unwrap();
            let client = NetworkAuthorityClient::new(channel);
            authority_clients.insert(validator.public_key(), client);
        }

        let active_authority = ActiveAuthority::new(state.clone(), authority_clients)?;

        // Start following validators
        tokio::task::spawn(async move {
            gossip_process(
                &active_authority,
                // listen to all authorities (note that gossip_process caps this to total minus 1.)
                active_authority.state.committee.voting_rights.len(),
            )
            .await;
        });

        // Start batch system so the full node can be followed - currently only the
        // tests use this, in order to wait until the full node has seen a tx.
        // However, there's no reason full nodes won't want to follow other full nodes
        // eventually.
        let batch_state = state.clone();
        tokio::task::spawn(async move {
            batch_state
                .run_batch_service(1000, Duration::from_secs(1))
                .await
        });

        info!("Started full node ");

        Ok(Self { state })
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
                .state
                .get_owned_objects(owner)
                .await
                .map_err(|e| anyhow!("{}", e))?
                .iter()
                .map(|w| SuiObjectRef::from(*w))
                .collect(),
        };
        Ok(resp)
    }

    async fn get_object_info(&self, object_id: ObjectID) -> RpcResult<GetObjectInfoResponse> {
        Ok(self
            .state
            .get_object_info(&object_id)
            .await
            .map_err(|e| anyhow!("{}", e))?
            .try_into()
            .map_err(|e| anyhow!("{}", e))?)
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.state.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_in_range(start, end)?)
    }

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_recent_transactions(count)?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse> {
        Ok(self.state.get_transaction(digest).await?)
    }

    async fn get_transactions_by_input_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_by_input_object(object).await?)
    }

    async fn get_transactions_by_mutated_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .state
            .get_transactions_by_mutated_object(object)
            .await?)
    }

    async fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_from_addr(addr).await?)
    }

    async fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_to_addr(addr).await?)
    }
}
