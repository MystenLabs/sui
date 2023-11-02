// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store::AuthorityStore;
use itertools::izip;
use once_cell::unsync::OnceCell;
use std::collections::HashMap;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber, TransactionDigest},
    error::{SuiError, SuiResult, UserInputError},
    fp_ensure,
    storage::{BackingPackageStore, GetSharedLocks, ObjectKey, ObjectStore},
    transaction::{
        InputObjectKind, InputObjects, ObjectReadResult, ObjectReadResultKind,
        ReceivingObjectReadResult, ReceivingObjectReadResultKind, ReceivingObjects,
    },
};

pub(crate) struct TransactionInputLoader {
    store: Arc<AuthorityStore>,
}

impl TransactionInputLoader {
    pub fn new(store: Arc<AuthorityStore>) -> Self {
        Self { store }
    }
}

impl TransactionInputLoader {
    /// Read the inputs for a transaction that the validator was asked to sign.
    /// tx_digest is provided so that the inputs can be cached with the tx_digest and returned with
    /// a single hash map lookup when notify_read_objects_for_execution is called later.
    pub async fn read_objects_for_signing(
        &self,
        _tx_digest: &TransactionDigest,
        input_object_kinds: &[InputObjectKind],
        receiving_objects: &[ObjectRef],
        protocol_config: &ProtocolConfig,
        epoch_id: EpochId,
    ) -> SuiResult<(InputObjects, ReceivingObjects)> {
        fp_ensure!(
            receiving_objects.len() + input_object_kinds.len()
                <= protocol_config.max_input_objects() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum input and receiving objects in a transaction".to_string(),
                value: protocol_config.max_input_objects().to_string()
            }
            .into()
        );

        let mut results = vec![None; input_object_kinds.len()];
        let mut object_keys = Vec::with_capacity(input_object_kinds.len());
        let mut fetch_indices = Vec::with_capacity(input_object_kinds.len());
        let mut missing_shared_objects = Vec::new();

        for (i, kind) in input_object_kinds.iter().enumerate() {
            let obj_ref = match kind {
                // Packages are loaded one at a time via the cache
                InputObjectKind::MovePackage(id) => {
                    let package = self.store.get_package_object(id)?.map(|o| o.into());
                    if package.is_none() {
                        return Err(SuiError::from(kind.object_not_found_error()));
                    }
                    results[i] = Some(ObjectReadResult {
                        input_object_kind: *kind,
                        object: ObjectReadResultKind::Object(package.unwrap()),
                    });
                    continue;
                }
                InputObjectKind::SharedMoveObject { id, .. } => {
                    let objref = self.store.get_latest_object_ref(*id)?;
                    if objref.is_none() {
                        missing_shared_objects.push((i, *id));
                        continue;
                    }
                    objref
                }
                InputObjectKind::ImmOrOwnedMoveObject(objref) => Some(*objref),
            }
            .ok_or_else(|| SuiError::from(kind.object_not_found_error()))?;

            object_keys.push(ObjectKey::from(obj_ref));
            fetch_indices.push(i);
        }

        let objects = self.store.multi_get_object_by_key(&object_keys)?;
        for (index, object) in fetch_indices.into_iter().zip(objects.into_iter()) {
            let object = object.ok_or_else(|| {
                SuiError::from(input_object_kinds[index].object_not_found_error())
            })?;

            results[index] = Some(ObjectReadResult {
                input_object_kind: input_object_kinds[index],
                object: ObjectReadResultKind::Object(Arc::new(object)),
            });
        }

        for (i, id) in missing_shared_objects {
            if let Some((version, digest)) = self
                .store
                .get_last_shared_object_deletion_info(&id, epoch_id)?
            {
                results[i] = Some(ObjectReadResult {
                    input_object_kind: input_object_kinds[i],
                    object: ObjectReadResultKind::DeletedSharedObject(version, digest),
                });
            } else {
                return Err(SuiError::from(
                    input_object_kinds[i].object_not_found_error(),
                ));
            }
        }

        // Load receiving objects
        let mut receiving_results = Vec::with_capacity(receiving_objects.len());
        for objref in receiving_objects {
            let (object_id, version, _) = objref;
            fp_ensure!(
                *version < SequenceNumber::MAX,
                UserInputError::InvalidSequenceNumber.into()
            );

            if self
                .store
                .have_received_object_at_version(object_id, *version, epoch_id)?
            {
                receiving_results.push(ReceivingObjectReadResult::new(
                    *objref,
                    ReceivingObjectReadResultKind::PreviouslyReceivedObject,
                ));
                continue;
            }

            let Some(object) = self.store.get_object(object_id)? else {
                return Err(UserInputError::ObjectNotFound {
                    object_id: *object_id,
                    version: Some(*version),
                }
                .into());
            };

            receiving_results.push(ReceivingObjectReadResult::new(*objref, object.into()));
        }

        Ok((
            results
                .into_iter()
                .map(Option::unwrap)
                .collect::<Vec<_>>()
                .into(),
            receiving_results.into(),
        ))
    }

    /// Reads input objects assuming a synchronous context such as the end of epoch transaction.
    /// By "synchronous" we mean that it is safe to read the latest version of all shared objects,
    /// as opposed to relying on the shared input version assignment.
    pub async fn read_objects_for_synchronous_execution(
        &self,
        tx_digest: &TransactionDigest,
        input_object_kinds: &[InputObjectKind],
        protocol_config: &ProtocolConfig,
    ) -> SuiResult<InputObjects> {
        self.read_objects_for_synchronous_execution_impl(
            Some(tx_digest),
            input_object_kinds,
            &[],
            protocol_config,
        )
        .await
        .map(|r| r.0)
    }

    /// Read input objects for a dry run execution.
    pub async fn read_objects_for_dry_run_exec(
        &self,
        tx_digest: &TransactionDigest,
        input_object_kinds: &[InputObjectKind],
        receiving_objects: &[ObjectRef],
        protocol_config: &ProtocolConfig,
    ) -> SuiResult<(InputObjects, ReceivingObjects)> {
        self.read_objects_for_synchronous_execution_impl(
            Some(tx_digest),
            input_object_kinds,
            receiving_objects,
            protocol_config,
        )
        .await
    }

    /// Read input objects for dev inspect
    pub async fn read_objects_for_dev_inspect(
        &self,
        input_object_kinds: &[InputObjectKind],
        receiving_objects: &[ObjectRef],
        protocol_config: &ProtocolConfig,
    ) -> SuiResult<(InputObjects, ReceivingObjects)> {
        self.read_objects_for_synchronous_execution_impl(
            None,
            input_object_kinds,
            receiving_objects,
            protocol_config,
        )
        .await
    }

    /// Read the inputs for a transaction that is ready to be executed.
    ///
    /// shared_lock_store is used to resolve the versions of any shared input objects.
    ///
    /// This function panics if any inputs are not available, as TransactionManager should already
    /// have verified that the transaction is ready to be executed.
    ///
    /// The tx_digest is provided here to support the following optimization (not yet implemented):
    /// All the owned input objects will likely have been loaded during transaction signing, and
    /// can be stored as a group with the transaction_digest as the key, allowing the lookup to
    /// proceed with only a single hash map lookup. (additional lookups may be necessary for shared
    /// inputs, since the versions are not known at signing time).
    pub async fn read_objects_for_execution(
        &self,
        shared_lock_store: &dyn GetSharedLocks,
        tx_digest: &TransactionDigest,
        input_object_kinds: &[InputObjectKind],
        epoch_id: EpochId,
    ) -> SuiResult<InputObjects> {
        let shared_locks_cell: OnceCell<HashMap<_, _>> = OnceCell::new();

        let mut results = vec![None; input_object_kinds.len()];
        let mut object_keys = Vec::with_capacity(input_object_kinds.len());
        let mut fetches = Vec::with_capacity(input_object_kinds.len());

        for (i, input) in input_object_kinds.iter().enumerate() {
            match input {
                InputObjectKind::MovePackage(id) => {
                    let package = self.store.get_package_object(id)?.unwrap_or_else(|| {
                        panic!(
                            "Executable transaction {:?} depends on non-existent package {:?}",
                            tx_digest, id
                        )
                    });

                    results[i] = Some(ObjectReadResult {
                        input_object_kind: *input,
                        object: ObjectReadResultKind::Object(package.into()),
                    });
                    continue;
                }
                InputObjectKind::ImmOrOwnedMoveObject(objref) => object_keys.push(objref.into()),
                InputObjectKind::SharedMoveObject { id, .. } => {
                    let shared_locks = shared_locks_cell.get_or_try_init(|| {
                        Ok::<HashMap<ObjectID, SequenceNumber>, SuiError>(
                            shared_lock_store
                                .get_shared_locks(tx_digest)?
                                .into_iter()
                                .collect(),
                        )
                    })?;
                    // If we can't find the locked version, it means
                    // 1. either we have a bug that skips shared object version assignment
                    // 2. or we have some DB corruption
                    let version = shared_locks.get(id).unwrap_or_else(|| {
                        panic!(
                            "Shared object locks should have been set. tx_digest: {:?}, obj id: {:?}",
                            tx_digest, id
                        )
                    });
                    object_keys.push(ObjectKey(*id, *version));
                    fetches.push((i, input));
                }
            }
        }

        let objects = self.store.multi_get_object_by_key(&object_keys)?;

        assert!(objects.len() == object_keys.len() && objects.len() == fetches.len());

        for (object, key, (index, input)) in izip!(
            objects.into_iter(),
            object_keys.into_iter(),
            fetches.into_iter()
        ) {
            results[index] = Some(match (object, input) {
                (Some(obj), input_object_kind) => ObjectReadResult {
                    input_object_kind: *input_object_kind,
                    object: obj.into(),
                },
                (None, InputObjectKind::SharedMoveObject { id, .. }) => {
                    // Check if the object was deleted by a concurrently certified tx
                    let version = key.1;
                    if let Some(dependency) = self.store.get_deleted_shared_object_previous_tx_digest(id, &version, epoch_id)? {
                        ObjectReadResult {
                            input_object_kind: *input,
                            object: ObjectReadResultKind::DeletedSharedObject(version, dependency),
                        }
                    } else {
                        panic!("All dependencies of tx {:?} should have been executed now, but Shared Object id: {}, version: {} is absent in epoch {}", tx_digest, *id, version, epoch_id);
                    }
                },
                _ => panic!("All dependencies of tx {:?} should have been executed now, but obj {:?} is absent", tx_digest, key),
            });
        }

        Ok(results
            .into_iter()
            .map(Option::unwrap)
            .collect::<Vec<_>>()
            .into())
    }
}

// private methods
impl TransactionInputLoader {
    async fn read_objects_for_synchronous_execution_impl(
        &self,
        _tx_digest: Option<&TransactionDigest>,
        input_object_kinds: &[InputObjectKind],
        receiving_objects: &[ObjectRef],
        protocol_config: &ProtocolConfig,
    ) -> SuiResult<(InputObjects, ReceivingObjects)> {
        fp_ensure!(
            receiving_objects.len() + input_object_kinds.len()
                <= protocol_config.max_input_objects() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum input and receiving objects in a transaction".to_string(),
                value: protocol_config.max_input_objects().to_string()
            }
            .into()
        );

        let mut results = Vec::with_capacity(input_object_kinds.len());
        for kind in input_object_kinds {
            let obj = match kind {
                InputObjectKind::MovePackage(id) => self
                    .store
                    .get_package_object(id)?
                    .map(|o| o.object().clone()),

                InputObjectKind::SharedMoveObject { id, .. } => self.store.get_object(id)?,
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    self.store.get_object_by_key(&objref.0, objref.1)?
                }
            }
            .ok_or_else(|| SuiError::from(kind.object_not_found_error()))?;
            results.push(ObjectReadResult::new(*kind, obj.into()));
        }
        Ok((results.into(), vec![].into()))
    }
}
