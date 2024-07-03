// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::effects::{IDOperation, ObjectIn, ObjectOut, TransactionEffects, TransactionEvents};
use crate::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointContents};
use crate::object::Object;
use crate::storage::BackingPackageStore;
use crate::transaction::Transaction;
use itertools::Either;
use serde::{Deserialize, Serialize};
use tap::Pipe;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointData {
    pub checkpoint_summary: CertifiedCheckpointSummary,
    pub checkpoint_contents: CheckpointContents,
    pub transactions: Vec<CheckpointTransaction>,
}

impl CheckpointData {
    pub fn output_objects(&self) -> Vec<&Object> {
        self.transactions
            .iter()
            .flat_map(|tx| &tx.output_objects)
            .collect()
    }

    pub fn input_objects(&self) -> Vec<&Object> {
        self.transactions
            .iter()
            .flat_map(|tx| &tx.input_objects)
            .collect()
    }

    pub fn all_objects(&self) -> Vec<&Object> {
        self.transactions
            .iter()
            .flat_map(|tx| &tx.input_objects)
            .chain(self.transactions.iter().flat_map(|tx| &tx.output_objects))
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointTransaction {
    /// The input Transaction
    pub transaction: Transaction,
    /// The effects produced by executing this transaction
    pub effects: TransactionEffects,
    /// The events, if any, emitted by this transaciton during execution
    pub events: Option<TransactionEvents>,
    /// The state of all inputs to this transaction as they were prior to execution.
    pub input_objects: Vec<Object>,
    /// The state of all output objects created or mutated by this transaction.
    pub output_objects: Vec<Object>,
}

impl CheckpointTransaction {
    // provide an iterator over all deleted or wrapped objects in this transaction
    pub fn removed_objects(&self) -> impl Iterator<Item = &Object> {
        // Iterator over id and versions for all deleted or wrapped objects
        match &self.effects {
            TransactionEffects::V1(v1) => Either::Left(
                // Effects v1 has delted and wrapped objects versions as the "new" version, not the
                // old one that was actually removed. So we need to take these and then look them
                // up in the `modified_at_versions`.
                v1.deleted().iter().chain(v1.wrapped()).map(|(id, _, _)| {
                    // lookup the old version for mutated objects
                    let (_, old_version) = v1
                        .modified_at_versions()
                        .iter()
                        .find(|(oid, _old_version)| oid == id)
                        .expect("deleted/wrapped object must have entry in 'modified_at_versions'");
                    (id, old_version)
                }),
            ),
            TransactionEffects::V2(v2) => {
                Either::Right(v2.changed_objects().iter().filter_map(|(id, change)| {
                    match (
                        &change.input_state,
                        &change.output_state,
                        &change.id_operation,
                    ) {
                        // Deleted Objects
                        (
                            ObjectIn::Exist(((version, _d), _o)),
                            ObjectOut::NotExist,
                            IDOperation::Deleted,
                        ) => Some((id, version)),

                        // Wrapped Objects
                        (
                            ObjectIn::Exist(((version, _), _)),
                            ObjectOut::NotExist,
                            IDOperation::None,
                        ) => Some((id, version)),
                        _ => None,
                    }
                }))
            }
        }
        // Use id and version to lookup in input Objects
        .map(|(id, version)| {
            self.input_objects
                .iter()
                .find(|o| &o.id() == id && &o.version() == version)
                .expect("all removed objects should show up in input objects")
        })
    }

    pub fn changed_objects(&self) -> impl Iterator<Item = (&Object, Option<&Object>)> {
        // Iterator over ((ObjectId, new version), Option<old version>)
        match &self.effects {
            TransactionEffects::V1(v1) => Either::Left(
                v1.created()
                    .iter()
                    .chain(v1.unwrapped())
                    .map(|((id, version, _), _)| ((id, version), None))
                    .chain(v1.mutated().iter().map(|((id, version, _), _)| {
                        // lookup the old version for mutated objects
                        let (_, old_version) = v1
                            .modified_at_versions()
                            .iter()
                            .find(|(oid, _old_version)| oid == id)
                            .expect("mutated object must have entry in modified_at_versions");

                        ((id, version), Some(old_version))
                    })),
            ),
            TransactionEffects::V2(v2) => {
                Either::Right(v2.changed_objects().iter().filter_map(|(id, change)| {
                    match (
                        &change.input_state,
                        &change.output_state,
                        &change.id_operation,
                    ) {
                        // Created Objects
                        (ObjectIn::NotExist, ObjectOut::ObjectWrite(_), IDOperation::Created) => {
                            Some(((id, &v2.lamport_version), None))
                        }
                        (
                            ObjectIn::NotExist,
                            ObjectOut::PackageWrite((version, _)),
                            IDOperation::Created,
                        ) => Some(((id, &version), None)),

                        // Unwrapped Objects
                        (ObjectIn::NotExist, ObjectOut::ObjectWrite(_), IDOperation::None) => {
                            Some(((id, &v2.lamport_version), None))
                        }

                        // Mutated Objects
                        (ObjectIn::Exist(((old_version, _), _)), ObjectOut::ObjectWrite(_), _) => {
                            Some(((id, &v2.lamport_version), Some(old_version)))
                        }
                        (
                            ObjectIn::Exist(((old_version, _), _)),
                            ObjectOut::PackageWrite((version, _)),
                            _,
                        ) => Some(((id, version), Some(old_version))),

                        _ => None,
                    }
                }))
            }
        }
        // Lookup Objects in output Objects as well as old versions for mutated objects
        .map(|((id, version), old_version)| {
            let object = self
                .output_objects
                .iter()
                .find(|o| &o.id() == id && &o.version() == version)
                .expect("changed objects should show up in output objects");

            let old_object = old_version.map(|old_version| {
                self.input_objects
                    .iter()
                    .find(|o| &o.id() == id && &o.version() == old_version)
                    .expect("mutated objects should have a previous version in input objects")
            });

            (object, old_object)
        })
    }

    pub fn created_objects(&self) -> impl Iterator<Item = &Object> {
        // Iterator over (ObjectId, version) for created objects
        match &self.effects {
            TransactionEffects::V1(v1) => Either::Left(
                v1.created()
                    .iter()
                    .map(|((id, version, _), _)| (id, version)),
            ),
            TransactionEffects::V2(v2) => {
                Either::Right(v2.changed_objects().iter().filter_map(|(id, change)| {
                    match (
                        &change.input_state,
                        &change.output_state,
                        &change.id_operation,
                    ) {
                        // Created Objects
                        (ObjectIn::NotExist, ObjectOut::ObjectWrite(_), IDOperation::Created) => {
                            Some((id, &v2.lamport_version))
                        }
                        (
                            ObjectIn::NotExist,
                            ObjectOut::PackageWrite((version, _)),
                            IDOperation::Created,
                        ) => Some((id, &version)),

                        _ => None,
                    }
                }))
            }
        }
        // Lookup Objects in output Objects as well as old versions for mutated objects
        .map(|(id, version)| {
            self.output_objects
                .iter()
                .find(|o| &o.id() == id && &o.version() == version)
                .expect("created objects should show up in output objects")
        })
    }
}

impl BackingPackageStore for CheckpointData {
    fn get_package_object(
        &self,
        package_id: &crate::base_types::ObjectID,
    ) -> crate::error::SuiResult<Option<crate::storage::PackageObject>> {
        self.transactions
            .iter()
            .flat_map(|transaction| transaction.output_objects.iter())
            .find(|object| object.is_package() && &object.id() == package_id)
            .cloned()
            .map(crate::storage::PackageObject::new)
            .pipe(Ok)
    }
}
