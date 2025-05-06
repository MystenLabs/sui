// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    errors::ReplayError,
    gql_queries::epoch_data::query,
    replay_interface::{
        EpochData, EpochStore, ObjectKey, ObjectStore, TransactionStore, VersionQuery,
    },
    Node,
};

use cynic::GraphQlResponse;
use cynic::Operation;
use std::{cell::RefCell, collections::BTreeMap};
use sui_types::{digests::TransactionDigest};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    committee::ProtocolVersion,
    effects::TransactionEffects,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
    transaction::TransactionData,
};
use tokio::runtime::Runtime;
use tracing::debug;

//
// DataStore and traits implementations
//

type EpochId = u64;

// Wrapper around the GQL client.
pub struct DataStore {
    rpc: reqwest::Url,
    client: reqwest::Client,
    rt: Runtime,
    node: Node,
    epoch_map: RefCell<BTreeMap<EpochId, EpochData>>,
}

impl TransactionStore for DataStore {
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<(TransactionData, TransactionEffects), ReplayError> {
        let digest = tx_digest.parse().map_err(|e| {
            let err = format!("{:?}", e);
            ReplayError::DataConversionError { err }
        })?;
        debug!("Start transaction_data_and_effects");
        let txn_data_and_effects = self.rt.block_on(async {
            tokio::try_join!(
                self.transaction_data(digest),
                self.transaction_effects(digest)
            )
        });
        debug!("End transaction_data_and_effects");
        txn_data_and_effects
    }
}

impl EpochStore for DataStore {
    fn epoch_info(&self, epoch: u64) -> Result<EpochData, ReplayError> {
        println!("DARIO: epoch_info({})", epoch);
        if let Some(epoch_data) = self.epoch_map.borrow().get(&epoch) {
            println!("DARIO: epoch_info found {:?}", epoch_data);
            return Ok(epoch_data.clone());
        }
        let epoch_data = self.rt.block_on(self.epoch(epoch))?;
        println!("DARIO: epoch_info loaded {:?}", epoch_data);
        self.epoch_map.borrow_mut().insert(epoch, epoch_data.clone());
        Ok(epoch_data)
    }

    // Get the protocol config for an epoch
    fn protocol_config(&self, epoch: u64) -> Result<ProtocolConfig, ReplayError> {
        let epoch = self.epoch_info(epoch)?;
        Ok(ProtocolConfig::get_for_version(
            ProtocolVersion::new(epoch.protocol_version),
            self.chain(),
        ))
    }
}

impl ObjectStore for DataStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Object>, ReplayError> {
        // TODO: rewire to the new schema once there
        keys.into_iter()
            .map(|key| match key.version_query {
                VersionQuery::Version(version) => self
                    .rt
                    .block_on(self.get_object_at_version(key.object_id, version)),
                VersionQuery::RootVersion(root_version) => self
                    .rt
                    .block_on(self.get_object_at_root_version(key.object_id, root_version)),
                VersionQuery::AtCheckpoint(checkpoint) => self
                    .rt
                    .block_on(self.get_object_at_checkpoint(key.object_id, checkpoint)),
                VersionQuery::ImmutableOrLatest => {
                    self.rt.block_on(self.get_latest_object(key.object_id))
                }
            })
            .collect()
    }
}

impl DataStore {
    pub fn new(node: Node) -> Result<Self, ReplayError> {
        let rpc = if let Node::Custom(ref url) = node {
            reqwest::Url::parse(url).map_err(|err| ReplayError::DataConversionError {
                err: format!("Failed to parse custom RPC URL: {}", err),
            })?
        } else {
            panic!("yeah yeah yeah")
        };
        let client = reqwest::Client::new();

        Ok(Self {
            client,
            rt: Runtime::new().unwrap(),
            node,
            epoch_map: RefCell::new(BTreeMap::new()),
            rpc,
        })
    }

    pub fn node(&self) -> &Node {
        &self.node
    }

    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    // internal API
    pub async fn run_query<T, V>(
        &self,
        operation: &Operation<T, V>,
    ) -> Result<GraphQlResponse<T>, ReplayError>
    where
        T: serde::de::DeserializeOwned,
        V: serde::Serialize,
    {
        println!("DARIO: run_query()");
        let res = self
            .client
            .post(self.rpc.clone());
        println!("DARIO: run_query() after post");
        let res = res
            .json(&operation);
        println!("DARIO: run_query() after post/json");
        let res = res
            .send()
            .await
            .map_err(|err| ReplayError::RPCError {
                err: format!("Failed to send request: {}", err),
            })?;
        println!("DARIO: run_query() after post/json/send");
        println!("DARIO: run_query() response.status() = {:?}", res.status());
        println!("DARIO: run_query() response.text() = {:?}", res.text().await.unwrap());
        panic!("AAAARRRRRGGGGGHHHHHH");
        // let res = res
        //     .json::<GraphQlResponse<T>>()
        //     .await
        //     .map_err(|err| ReplayError::RPCError {
        //         err: format!("Failed to parse response: {}", err),
        //     })?;
        // println!("DARIO: run_query() after post/json/send/json");
        // // let res = self
        // //     .client
        // //     .post(self.rpc.clone())
        // //     .json(&operation)
        // //     .send()
        // //     .await
        // //     .map_err(|err| ReplayError::RPCError {
        // //         err: format!("Failed to send request: {}", err),
        // //     })?
        // //     .json::<GraphQlResponse<T>>()
        // //     .await
        // //     .map_err(|err| ReplayError::RPCError {
        // //         err: format!("Failed to parse response: {}", err),
        // //     })?;
        // Ok(res)
    }

    //
    // Transaction data and effects
    //

    async fn transaction_data(
        &self,
        _digest: TransactionDigest,
    ) -> Result<TransactionData, ReplayError> {
        todo!()
    }

    async fn transaction_effects(
        &self,
        _digest: TransactionDigest,
    ) -> Result<TransactionEffects, ReplayError> {
        todo!()
    }

    //
    // Epoch table API
    //

    // load the entire epoch table at once
    async fn epoch_table(&self) -> Result<(), ReplayError> {
        todo!()
    }

    async fn epoch(&self, epoch_id: u64) -> Result<EpochData, ReplayError> {
        query(epoch_id, self).await
    }

    //
    // Object loading API
    //

    async fn get_object_at_version(
        &self,
        _object_id: ObjectID,
        _version: u64,
    ) -> Result<Object, ReplayError> {
        todo!()
    }

    async fn get_object_at_root_version(
        &self,
        _object_id: ObjectID,
        _root_version: u64,
    ) -> Result<Object, ReplayError> {
        todo!()
    }

    async fn get_object_at_checkpoint(
        &self,
        _object_id: ObjectID,
        _checkpoint: u64,
    ) -> Result<Object, ReplayError> {
        todo!()
    }

    async fn get_latest_object(&self, _object_id: ObjectID) -> Result<Object, ReplayError> {
        todo!()
    }

    /// Load all versions of a system package with the given id.
    pub async fn load_system_package(
        &self,
        _pkg_id: ObjectID,
    ) -> Result<Vec<(Object, u64)>, ReplayError> {
        todo!()
    }

    /// Load all versions of all system packages
    pub async fn load_system_packages(
        &self,
    ) -> Result<BTreeMap<ObjectID, BTreeMap<u64, Object>>, ReplayError> {
        todo!()
    }

    //
    // Dynamic fields operations
    //

    pub fn read_child_object(
        &self,
        _parent: &ObjectID,
        _child: &ObjectID,
        _child_version_upper_bound: SequenceNumber,
    ) -> Result<Option<Object>, ReplayError> {
        todo!()
    }
}
