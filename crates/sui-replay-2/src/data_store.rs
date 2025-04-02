// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    errors::ReplayError,
    gql_queries::{package_versions_for_replay, EpochData},
    Node,
};

use std::collections::{BTreeMap, BTreeSet};
use sui_graphql_client::{Client, Direction, PaginationFilter};
use sui_sdk_types::Address;
use sui_types::{
    base_types::ObjectID, effects::TransactionEffects, move_package::MovePackage, object::Object,
    supported_protocol_versions::Chain, transaction::TransactionData,
};
use tracing::debug;

//
// Macro helpers
//
macro_rules! from_package {
    ($pkg:expr, $pkg_address:expr) => {
        MovePackage::try_from($pkg).map_err(|e| ReplayError::PackageConversionError {
            pkg: $pkg_address.to_string(),
            err: format!("{:?}", e),
        })
    };
}

// Wrapper around the GQL client.
pub struct DataStore {
    client: Client,
    node: Node,
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
        Ok(Self { client, node })
    }

    pub fn node(&self) -> &Node {
        &self.node
    }

    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    //
    // Package operations
    //

    /// Load a package with the given id.
    pub async fn get_package(&self, pkg_id: ObjectID) -> Result<MovePackage, ReplayError> {
        debug!("get_package: {}", pkg_id);
        let pkg_address = Address::new(pkg_id.into_bytes());
        let pkg = self
            .client
            .package(pkg_address, None)
            .await
            .map_err(|e| ReplayError::LoadPackageError {
                pkg: pkg_address.to_string(),
                err: format!("{:?}", e),
            })?
            .ok_or_else(|| ReplayError::PackageNotFound {
                pkg: pkg_address.to_string(),
            })?;
        from_package!(pkg, pkg_address)
    }

    /// Load all versions of the packages with the given id.
    /// The id can be any package at any version.
    pub async fn get_system_package(
        &self,
        pkg_id: ObjectID,
    ) -> Result<Vec<(Object, u64)>, ReplayError> {
        debug!("get_system_package: {}", pkg_id);
        let pkg_address = Address::new(pkg_id.into_bytes());
        let mut packages = vec![];
        let mut pagination = PaginationFilter {
            direction: Direction::Forward,
            cursor: None,
            limit: None,
        };
        loop {
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

                debug!(
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
        Ok(packages)
    }

    /// Load all versions of all system packages
    pub async fn get_system_packages(
        &self,
    ) -> Result<BTreeMap<ObjectID, BTreeMap<u64, Object>>, ReplayError> {
        debug!("Start get_system_packages");
        let mut system_packages = BTreeMap::new();
        for pkg_id in sui_framework::BuiltInFramework::all_package_ids() {
            let packages = self.get_system_package(pkg_id).await?;
            let all_versions = packages
                .into_iter()
                .map(|(pkg, epoch)| {
                    debug!("{}[{}] - {}", pkg.id(), pkg.version(), epoch);
                    (epoch, pkg)
                })
                .collect();
            system_packages.insert(pkg_id, all_versions);
        }
        debug!("End get_system_packages");
        Ok(system_packages)
    }

    //
    // Object operations
    //
    pub async fn load_objects(
        &self,
        object_inputs: &BTreeSet<InputObject>,
    ) -> Result<Vec<(ObjectID, Option<u64>, Object)>, ReplayError> {
        let mut objects = vec![];
        for object_input in object_inputs {
            let InputObject {
                object_id, version, ..
            } = object_input;
            let address = Address::new((*object_id).into_bytes());
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
        Ok(objects)
    }

    //
    // Transaction operations
    //

    pub async fn transaction_data(&self, tx_digest: &str) -> Result<TransactionData, ReplayError> {
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
        Ok(txn_data)
    }

    pub async fn transaction_effects(
        &self,
        tx_digest: &str,
    ) -> Result<TransactionEffects, ReplayError> {
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
        Ok(effects)
    }

    //
    // Epoch store load
    //
    pub async fn epochs_gql_table(&self) -> Result<BTreeMap<u64, EpochData>, ReplayError> {
        let mut pag_filter = PaginationFilter {
            direction: Direction::Backward,
            cursor: None,
            limit: None,
        };

        let mut epochs_data = BTreeMap::<u64, EpochData>::new();
        loop {
            let paged_epochs = crate::gql_queries::epochs(&self.client, pag_filter)
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

        Ok(epochs_data)
    }
}
