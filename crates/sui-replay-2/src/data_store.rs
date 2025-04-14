// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    errors::ReplayError,
    gql_queries::{dynamic_field_at_version, package_versions_for_replay, EpochData},
    Node,
};

use std::collections::{BTreeMap, BTreeSet};
use sui_graphql_client::{Client, Direction, PaginationFilter};
use sui_sdk_types::Address;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    committee::ProtocolVersion,
    effects::TransactionEffects,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
    transaction::TransactionData,
};
use tracing::{debug, trace};

// Wrapper around the GQL client.
pub struct DataStore {
    client: Client,
    node: Node,
    epoch_store: EpochStore,
}

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub struct InputObject {
    pub object_id: ObjectID,
    pub version: Option<u64>,
}

impl DataStore {
    pub fn new(node: Node) -> Result<Self, ReplayError> {
        let client = match &node {
            Node::Mainnet => Client::new_mainnet(),
            Node::Testnet => Client::new_testnet(),
            Node::Devnet => Client::new_devnet(),
            Node::Custom(host) => {
                Client::new(host).map_err(|e| ReplayError::ClientCreationError {
                    host: format!("{:?}", &host),
                    err: format!("{:?}", e),
                })?
            }
        };
        let epoch_store = EpochStore::lazy();
        Ok(Self {
            client,
            node,
            epoch_store,
        })
    }

    pub async fn new_eager(node: Node) -> Result<Self, ReplayError> {
        let client = match &node {
            Node::Mainnet => Client::new_mainnet(),
            Node::Testnet => Client::new_testnet(),
            Node::Devnet => Client::new_devnet(),
            Node::Custom(host) => {
                Client::new(host).map_err(|e| ReplayError::ClientCreationError {
                    host: format!("{:?}", &host),
                    err: format!("{:?}", e),
                })?
            }
        };
        let data = Self::epochs_gql_table(&client).await?;
        let epoch_store = EpochStore::eager(data);
        Ok(Self {
            client,
            node,
            epoch_store,
        })
    }

    pub fn node(&self) -> &Node {
        &self.node
    }

    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    pub fn protocol_config(&self, epoch: u64, chain: Chain) -> Result<ProtocolConfig, ReplayError> {
        self.epoch_store
            .protocol_config(epoch, chain)
            .map_err(|_err| todo!("hook epoch table lookup"))
    }

    pub fn epoch_timestamp(&self, epoch: u64) -> Result<u64, ReplayError> {
        self.epoch_store
            .epoch_timestamp(epoch)
            .map_err(|_err| todo!("hook epoch table lookup"))
    }

    pub fn rgp(&self, epoch: u64) -> Result<u64, ReplayError> {
        self.epoch_store
            .rgp(epoch)
            .map_err(|_err| todo!("hook epoch table lookup"))
    }

    /// Load all versions of a system package with the given id.
    pub async fn load_system_package(
        &self,
        pkg_id: ObjectID,
    ) -> Result<Vec<(Object, u64)>, ReplayError> {
        debug!("Start load system package: {}", pkg_id);
        let pkg_address = Address::new(pkg_id.into_bytes());
        let mut packages = vec![];
        let mut pagination = PaginationFilter {
            direction: Direction::Forward,
            cursor: None,
            limit: None,
        };
        loop {
            debug!("Start load system package pagination");
            let pkg_versions =
                package_versions_for_replay(&self.client, pkg_address, pagination, None, None)
                    .await
                    .map_err(|e| ReplayError::PackagesRetrievalError {
                        pkg: pkg_address.to_string(),
                        err: format!("{:?}", e),
                    })?;
            let (page_info, data) = pkg_versions.into_parts();
            for (pkg, pkg_version, epoch) in data {
                let package_obj = pkg.ok_or_else(|| ReplayError::PackageNotFound {
                    pkg: pkg_address.to_string(),
                })?;
                let package_obj = Object::try_from(package_obj).map_err(|e| {
                    ReplayError::ObjectConversionError {
                        id: pkg_address.to_string(),
                        err: format!("{:?}", e),
                    }
                })?;

                trace!(
                    "Collecting system package: {}[{} - {}], {:?}",
                    package_obj.id(),
                    package_obj.version(),
                    pkg_version,
                    epoch,
                );
                // TODO: fix this. If epoch is missing come up with something....
                let epoch = epoch.unwrap_or(0);
                // epoch.ok_or_else(|| ReplayError::MissingPackageEpoch {
                //     pkg: pkg_address.to_string(),
                // })? as u64;
                packages.push((package_obj, epoch));
            }
            if page_info.has_next_page {
                pagination = PaginationFilter {
                    direction: Direction::Forward,
                    cursor: page_info.end_cursor.clone(),
                    limit: None,
                };
            } else {
                break;
            }
        }
        debug!("End load system package");
        Ok(packages)
    }

    /// Load all versions of all system packages
    pub async fn load_system_packages(
        &self,
    ) -> Result<BTreeMap<ObjectID, BTreeMap<u64, Object>>, ReplayError> {
        debug!("Start load system packages");
        let mut system_packages = BTreeMap::new();
        for pkg_id in sui_framework::BuiltInFramework::all_package_ids() {
            let packages = self.load_system_package(pkg_id).await?;
            let all_versions = packages
                .into_iter()
                .map(|(pkg, epoch)| {
                    trace!("{}[{}] - {}", pkg.id(), pkg.version(), epoch);
                    (epoch, pkg)
                })
                .collect();
            system_packages.insert(pkg_id, all_versions);
        }
        debug!("End load system packages");
        Ok(system_packages)
    }

    // Load a set of (ObjectId, version) objects
    pub async fn load_objects(
        &self,
        object_inputs: &BTreeSet<InputObject>,
    ) -> Result<Vec<(ObjectID, Option<u64>, Object)>, ReplayError> {
        debug!("Start load objects");
        let mut objects = vec![];
        for object_input in object_inputs {
            let InputObject {
                object_id, version, ..
            } = object_input;
            let address = Address::new((*object_id).into_bytes());
            debug!("Start load object: {}", address);
            let object_data = self
                .client
                .object(address, *version)
                .await
                .map_err(|e| ReplayError::ObjectLoadError {
                    address: address.to_string(),
                    version: *version,
                    err: format!("{:?}", e),
                })?
                .ok_or_else(|| ReplayError::ObjectNotFound {
                    address: address.to_string(),
                    version: *version,
                })?;
            let object =
                Object::try_from(object_data).map_err(|e| ReplayError::ObjectConversionError {
                    id: object_id.to_string(),
                    err: format!("{:?}", e),
                })?;
            objects.push((*object_id, *version, object));
        }
        debug!("End load objects");
        Ok(objects)
    }

    //
    // Transaction data and effects
    //

    // get transaction data and effects
    pub async fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<(TransactionData, TransactionEffects), ReplayError> {
        let txn_data = self.transaction_data(tx_digest).await?;
        let txn_effects = self.transaction_effects(tx_digest).await?;
        Ok((txn_data, txn_effects))
    }

    async fn transaction_data(&self, tx_digest: &str) -> Result<TransactionData, ReplayError> {
        debug!("Start transaction data");
        let digest = tx_digest.parse().map_err(|e| {
            let err = format!("{:?}", e);
            let digest = tx_digest.to_string();
            ReplayError::FailedToParseDigest { digest, err }
        })?;
        let tx = self
            .client
            .transaction(digest)
            .await
            .map_err(|e| {
                let err = format!("{:?}", e);
                let digest = tx_digest.to_string();
                ReplayError::FailedToLoadTransaction { digest, err }
            })?
            .ok_or(ReplayError::TransactionNotFound {
                digest: tx_digest.to_string(),
                node: self.node.clone(),
            })?;
        let txn_data = TransactionData::try_from(tx.transaction).map_err(|e| {
            ReplayError::TransactionConversionError {
                id: tx_digest.to_string(),
                err: format!("{:?}", e),
            }
        })?;
        debug!("End transaction data");
        Ok(txn_data)
    }

    async fn transaction_effects(
        &self,
        tx_digest: &str,
    ) -> Result<TransactionEffects, ReplayError> {
        debug!("Start transaction effects");
        let digest = tx_digest.parse().map_err(|e| {
            let err = format!("{:?}", e);
            let digest = tx_digest.to_string();
            ReplayError::FailedToParseDigest { digest, err }
        })?;
        let effects = self
            .client
            .transaction_effects(digest)
            .await
            .map_err(|e| {
                let err = format!("{:?}", e);
                let digest = tx_digest.to_string();
                ReplayError::FailedToLoadTransactionEffects { digest, err }
            })?
            .ok_or(ReplayError::TransactionEffectsNotFound {
                digest: tx_digest.to_string(),
                node: self.node.clone(),
            })?;
        let effects = TransactionEffects::try_from(effects).map_err(|e| {
            ReplayError::TransactionEffectsConversionError {
                id: tx_digest.to_string(),
                err: format!("{:?}", e),
            }
        })?;
        debug!("End transaction effects");
        Ok(effects)
    }

    //
    // Epoch table eager load
    //
    async fn epochs_gql_table(client: &Client) -> Result<BTreeMap<u64, EpochData>, ReplayError> {
        debug!("Start load epoch table");
        let mut pag_filter = PaginationFilter {
            direction: Direction::Backward,
            cursor: None,
            limit: None,
        };

        let mut epochs_data = BTreeMap::<u64, EpochData>::new();
        loop {
            debug!("Start load epoch table pagination");
            let paged_epochs = crate::gql_queries::epochs(client, pag_filter)
                .await
                .map_err(|e| {
                    let err = format!("{:?}", e);
                    ReplayError::GenericError { err }
                })
                .unwrap();
            let (page_info, data) = paged_epochs.into_parts();
            for epoch in data {
                epochs_data.insert(epoch.epoch_id, epoch.try_into()?);
            }
            if page_info.has_previous_page {
                pag_filter = PaginationFilter {
                    direction: Direction::Backward,
                    cursor: page_info.start_cursor,
                    limit: None,
                };
            } else {
                break;
            }
        }

        debug!("End load epoch table");
        Ok(epochs_data)
    }

    //
    // Dynamic fields operations
    //

    pub fn read_child_object(
        &self,
        _parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> Result<Option<Object>, ReplayError> {
        #[allow(clippy::disallowed_methods)]
        let obj = futures::executor::block_on(dynamic_field_at_version(
            &self.client,
            *child,
            child_version_upper_bound.value(),
        ))
        .map_err(|e| ReplayError::DynamicFieldError { err: e.to_string() })?;
        match obj {
            None => Ok(None),
            Some(obj) => {
                let obj = Object::try_from(obj)
                    .map_err(|e| ReplayError::DynamicFieldError { err: e.to_string() })?;
                Ok(Some(obj))
            }
        }
    }
}

type EpochId = u64;

// Eager loading of the epoch table from GQL.
// Maps an epoch to data vital to trascation execution:
// framework versions, protocol version, RGP, epoch start timestamp
#[derive(Debug)]
pub struct EpochStore {
    data: BTreeMap<EpochId, EpochData>,
}

impl EpochStore {
    fn eager(data: BTreeMap<EpochId, EpochData>) -> Self {
        Self { data }
    }

    // TODO: implement `lazy` once the data is available in the indexer
    fn lazy() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    // Get the protocol config for an epoch
    fn protocol_config(&self, epoch: u64, chain: Chain) -> Result<ProtocolConfig, ReplayError> {
        let epoch = self
            .data
            .get(&epoch)
            .ok_or(ReplayError::MissingDataForEpoch {
                data: "ProtocolConfig".to_string(),
                epoch,
            })?;
        Ok(ProtocolConfig::get_for_version(
            ProtocolVersion::new(epoch.protocol_version),
            chain,
        ))
    }

    // Get the RGP for an epoch
    fn rgp(&self, epoch: u64) -> Result<u64, ReplayError> {
        let epoch = self
            .data
            .get(&epoch)
            .ok_or(ReplayError::MissingDataForEpoch {
                data: "RGP".to_string(),
                epoch,
            })?;
        Ok(epoch.rgp)
    }

    // Get the start timestamp for an epoch
    fn epoch_timestamp(&self, epoch: u64) -> Result<u64, ReplayError> {
        let epoch = self
            .data
            .get(&epoch)
            .ok_or(ReplayError::MissingDataForEpoch {
                data: "timestamp".to_string(),
                epoch,
            })?;
        Ok(epoch.start_timestamp)
    }
}
