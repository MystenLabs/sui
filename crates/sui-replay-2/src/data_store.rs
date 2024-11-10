// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{epoch_store::{EpochStoreEager, EpochStore}, errors::ReplayError, Node};
use std::collections::{BTreeMap, BTreeSet};
use sui_graphql_client::{
    query_types::EventFilter, 
    Client, Direction, PaginationFilter,
};
use sui_sdk_types::types::{
    Address, EndOfEpochTransactionKind, TransactionDigest as SdkTransactionDigest, 
    TransactionKind as SdkTransactionKind 
};
use sui_types::{
    base_types::ObjectID, 
    digests::TransactionDigest, 
    effects::TransactionEffects, 
    event::SystemEpochInfoEvent, 
    move_package::MovePackage, 
    object::Object, 
    supported_protocol_versions::Chain, 
    transaction::TransactionData,
};
use tracing::{debug, info};

const EPOCH_CHANGE_STRUCT_TAG: &str = "0x3::sui_system_state_inner::SystemEpochInfoEvent";

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
            Node::Custom(host) => 
                Client::new(host).map_err(
                    |e| ReplayError::ClientCreationError {
                        host: format!("{:?}", &host),
                        err: format!("{:?}", e),
                    }
                )?,
        };
        Ok(Self { client, node })
    }

    pub fn node(&self) -> &Node {
        &self.node
    }

    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    //
    // Package operations
    //

    /// Load a package with the given id.
    pub async fn get_package(
        &self,
        pkg_id: ObjectID,
    ) -> Result<MovePackage, ReplayError> {
        debug!("get_package: {}", pkg_id);
        let pkg_address = Address::new(pkg_id.into_bytes());
        let pkg = self
            .client
            .package(pkg_address, None)
            .await
            .map_err(|e| {
                ReplayError::LoadPackageError {
                    pkg: pkg_address.to_string(), 
                    err: format!("{:?}", e),
                }
            })?
            .ok_or_else(|| ReplayError::PackageNotFound{ pkg: pkg_address.to_string() })?;
        from_package!(pkg, pkg_address)  
    }

    /// Load all versions of the packages with the given id.
    /// The id can be any package at any version.
    pub async fn get_system_package(
        &self,
        pkg_id: ObjectID,
    ) -> Result<Vec<(MovePackage, u64)>, ReplayError> {
        debug!("get_packages: {}", pkg_id);
        let pkg_address = Address::new(pkg_id.into_bytes());
        let mut packages = vec![];
        let mut pagination = PaginationFilter {
            direction: Direction::Forward,
            cursor: None,
            limit: None,
        };
        loop {
            let pkg_versions = self
                .client
                .package_versions_for_replay(pkg_address, pagination, None, None)
                .await
                .map_err(|e| {
                    ReplayError::PackagesRetrievalError{ 
                        pkg: pkg_address.to_string(), 
                        err: format!("{:?}", e),
                    }
                })?;
            let (page_info, data) = pkg_versions.into_parts();
            for (pkg, pkg_version, epoch) in data {
                let package = from_package!(
                    pkg.ok_or_else(|| ReplayError::PackageNotFound{ pkg: pkg_address.to_string() })?,
                    pkg_address
                )?;
                info!(
                    "Collecting system package: {}[{} - {}], {:?}", 
                    package.id(), 
                    package.version(), 
                    pkg_version, 
                    epoch,
                );
                let epoch = epoch.unwrap_or(0);
                    // epoch.ok_or_else(|| ReplayError::MissingPackageEpoch { 
                    //     pkg: pkg_address.to_string(),
                    // })? as u64;
                debug!(
                    "{}[{}/{}] - {}", 
                    package.id(),
                    package.version(),
                    pkg_version,
                    epoch,
                );
                packages.push((package, epoch));
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
    ) -> Result<BTreeMap<ObjectID, BTreeMap<u64, MovePackage>>, ReplayError> {
        let mut system_packages = BTreeMap::new();
        for pkg_id in sui_framework::BuiltInFramework::all_package_ids() {
            let packages = self.get_system_package(pkg_id).await?;
            let all_versions = packages.into_iter().map(|(pkg, epoch)| {
                debug!("{}[{}] - {}", pkg.id(), pkg.version(), epoch);
                (epoch, MovePackage::from(pkg))
            }).collect();
            system_packages.insert(pkg_id, all_versions);
        }
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
            let InputObject {object_id, version, .. } = object_input;
            let address = Address::new(object_id.clone().into_bytes());
            let object_data = self
                .client
                .object(address, version.clone())
                .await
                .map_err(|e| {
                    ReplayError::ObjectLoadError {
                        address: address.to_string(),
                        version: version.clone(),
                        err: format!("{:?}", e),
                    }
                })?
                .ok_or_else(|| ReplayError::ObjectNotFound {
                    address: address.to_string(), 
                    version: version.clone(),
                })?;
            let object = Object::try_from(object_data).map_err(|e| {
                ReplayError::ObjectConversionError { 
                    id: object_id.to_string(),
                    err: format!("{:?}", e),
                }
            })?;
            objects.push((*object_id, version.clone(), object));
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
            ReplayError::FailedToParseDigest {digest, err}
        })?;
        let tx = self
            .client
            .transaction(digest)
            .await
            .map_err(|e| {
                let err = format!("{:?}", e);
                let digest = tx_digest.to_string();
                ReplayError::FailedToLoadTransaction {digest, err}
            })?
            .ok_or(
                ReplayError::TransactionNotFound {
                    digest: tx_digest.to_string(), 
                    node: self.node.clone(),
                }
            )?;
        let txn_data = TransactionData::try_from(tx.transaction)
            .map_err(|e| {
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
            ReplayError::FailedToParseDigest {digest, err}
        })?;
        let effects = self
            .client
            .transaction_effects(digest)
            .await
            .map_err(|e| {
                let err = format!("{:?}", e);
                let digest = tx_digest.to_string();
                ReplayError::FailedToLoadTransactionEffects {digest, err}
            })?
            .ok_or(
                ReplayError::TransactionEffectsNotFound {
                    digest: tx_digest.to_string(), 
                    node: self.node.clone(),
                }
            )?;
        let effects = TransactionEffects::try_from(effects)
            .map_err(|e| {
                ReplayError::TransactionEffectsConversionError {
                    id: tx_digest.to_string(), 
                    err: format!("{:?}", e),
                }
            })?;
        Ok(effects)
    }

    //
    // Epoch operations
    //
    pub async fn epoch_store(
        &self,
    ) -> Result<EpochStore, ReplayError> {
        let mut protocol_configs = vec![];
        let mut rgps = vec![];
        let mut epoch_info = BTreeMap::new();
    
        let filter = EventFilter {
            emitting_module: None,
            event_type: Some(EPOCH_CHANGE_STRUCT_TAG.to_string()),
            sender: None,
            transaction_digest: None,
        };
        let mut pagination = PaginationFilter {
            direction: Direction::Backward,
            cursor: None,
            limit: None,
        };
        let mut quit = 0;
        loop {
            if quit == 1 {
                break;
            } else {
                quit += 1;
            }
            let paged_events = self
                .client
                .events(Some(filter.clone()), pagination)
                .await
                .map_err(|e| {
                    let err = format!("{:?}", e);
                    ReplayError::ChangeEpochEventsFailure { err }
                })?;
            let (page_info, events) = paged_events.into_parts();
            for (event, digest) in events.into_iter().rev() {
                let change_epoch_event: SystemEpochInfoEvent =
                    bcs::from_bytes(&event.contents).unwrap();
                let epoch = change_epoch_event.epoch;
                let timestamp = self.get_epoch_timestamp(digest).await?;
                epoch_info.insert(epoch, (timestamp, TransactionDigest::from(digest)));
                if rgps.is_empty() {
                    rgps.push(
                        (change_epoch_event.reference_gas_price, epoch, epoch),
                    );
                } else {
                    let last = rgps.last_mut().unwrap();
                    if last.0 != change_epoch_event.reference_gas_price {
                        last.1 = epoch;
                        rgps.push((
                            change_epoch_event.reference_gas_price,
                            epoch,
                            epoch,
                        ));
                    }
                }
                if protocol_configs.is_empty() {
                    protocol_configs.push((
                        change_epoch_event.protocol_version,
                        epoch,
                        epoch,
                    ));
                } else {
                    let last = protocol_configs.last_mut().unwrap();
                    if last.0 != change_epoch_event.protocol_version {
                        last.1 = epoch;
                        protocol_configs.push((
                            change_epoch_event.protocol_version,
                            epoch,
                            epoch,
                        ));
                    }
                }
            }
            if page_info.has_previous_page {
                pagination = PaginationFilter {
                    direction: Direction::Backward,
                    cursor: page_info.start_cursor.clone(),
                    limit: None,
                };
            } else {
                break;
            }
        }
        protocol_configs.reverse();
        if let Some(ref mut protocol_config) = protocol_configs.get_mut(0) {
            protocol_config.1 = 0;
        }
        rgps.reverse();
        if let Some(ref mut rgp) = rgps.get_mut(0) {
            rgp.1 = 0;
        }

        Ok(EpochStore::EpochInfoEager(EpochStoreEager {
            protocol_configs,
            rgps,
            epoch_info,
        }))

        // Ok((protocol_configs, rgps, epoch_info))
    }

    async fn get_epoch_timestamp(
        &self,
        digest: SdkTransactionDigest,
    ) -> Result<u64, ReplayError> {
        let txn_info = self
            .client
            .transaction(digest.into())
            .await
            .map_err(|e| {
                let err = format!("{:?}", e);
                let digest = digest.to_string();
                ReplayError::FailedToLoadTransaction {digest, err}
            })?
            .ok_or(
                ReplayError::TransactionNotFound {
                    digest: digest.to_string(), 
                    node: self.node.clone(),
                }
            )?;
        match txn_info.transaction.kind {
            SdkTransactionKind::ChangeEpoch(change) => {
                return Ok(change.epoch_start_timestamp_ms)
            },
            SdkTransactionKind::EndOfEpoch(kinds) => {
                for kind in kinds {
                    match kind {
                        EndOfEpochTransactionKind::ChangeEpoch(change) =>
                            return Ok(change.epoch_start_timestamp_ms),
                        _ => ()
                    }
                }
            }
            _ => (),
        };
        Err(ReplayError::NoEpochTimestamp {digest: digest.to_string()})
    }
        
}
