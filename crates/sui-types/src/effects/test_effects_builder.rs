// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{ObjectID, SequenceNumber};
use crate::digests::{ObjectDigest, TransactionEventsDigest};
use crate::effects::{EffectsObjectChange, IDOperation, ObjectIn, ObjectOut, TransactionEffects};
use crate::execution::SharedInput;
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::message_envelope::Message;
use crate::object::Owner;
use crate::transaction::{InputObjectKind, SenderSignedData, TransactionDataAPI};
use std::collections::BTreeMap;

pub struct TestEffectsBuilder {
    transaction: SenderSignedData,
    /// Override the execution status if provided.
    status: Option<ExecutionStatus>,
    /// Provide the assigned versions for all shared objects.
    shared_input_versions: BTreeMap<ObjectID, SequenceNumber>,
    events_digest: Option<TransactionEventsDigest>,
}

impl TestEffectsBuilder {
    pub fn new(transaction: &SenderSignedData) -> Self {
        Self {
            transaction: transaction.clone(),
            status: None,
            shared_input_versions: BTreeMap::new(),
            events_digest: None,
        }
    }

    pub fn with_status(mut self, status: ExecutionStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_shared_input_versions(
        mut self,
        versions: BTreeMap<ObjectID, SequenceNumber>,
    ) -> Self {
        assert!(self.shared_input_versions.is_empty());
        self.shared_input_versions = versions;
        self
    }

    pub fn with_events_digest(mut self, digest: TransactionEventsDigest) -> Self {
        self.events_digest = Some(digest);
        self
    }

    pub fn build(self) -> TransactionEffects {
        let status = self.status.unwrap_or_else(|| ExecutionStatus::Success);
        // TODO: This does not yet support deleted shared objects.
        let shared_objects = self
            .shared_input_versions
            .iter()
            .map(|(id, version)| SharedInput::Existing((*id, *version, ObjectDigest::MIN)))
            .collect();
        let executed_epoch = 0;
        let lamport_version = SequenceNumber::lamport_increment(
            self.transaction
                .transaction_data()
                .input_objects()
                .unwrap()
                .iter()
                .filter_map(|kind| kind.version())
                .chain(
                    self.transaction
                        .transaction_data()
                        .receiving_objects()
                        .iter()
                        .map(|oref| oref.1),
                )
                .chain(self.shared_input_versions.values().copied()),
        );
        let sender = self.transaction.transaction_data().sender();
        // TODO: Include receiving objects in the object changes as well.
        let changed_objects = self
            .transaction
            .transaction_data()
            .input_objects()
            .unwrap()
            .iter()
            .filter_map(|kind| match kind {
                InputObjectKind::ImmOrOwnedMoveObject(oref) => Some((
                    oref.0,
                    EffectsObjectChange {
                        input_state: ObjectIn::Exist((
                            (oref.1, oref.2),
                            Owner::AddressOwner(sender),
                        )),
                        output_state: ObjectOut::ObjectWrite((
                            // Digest must change with a mutation.
                            ObjectDigest::MAX,
                            Owner::AddressOwner(sender),
                        )),
                        id_operation: IDOperation::None,
                    },
                )),
                InputObjectKind::MovePackage(_) => None,
                InputObjectKind::SharedMoveObject {
                    id,
                    initial_shared_version,
                    mutable,
                } => mutable.then_some((
                    *id,
                    EffectsObjectChange {
                        input_state: ObjectIn::Exist((
                            (
                                *self
                                    .shared_input_versions
                                    .get(id)
                                    .unwrap_or(initial_shared_version),
                                ObjectDigest::MIN,
                            ),
                            Owner::Shared {
                                initial_shared_version: *initial_shared_version,
                            },
                        )),
                        output_state: ObjectOut::ObjectWrite((
                            // Digest must change with a mutation.
                            ObjectDigest::MAX,
                            Owner::Shared {
                                initial_shared_version: *initial_shared_version,
                            },
                        )),
                        id_operation: IDOperation::None,
                    },
                )),
            })
            .collect();
        let gas_object_id = self.transaction.transaction_data().gas()[0].0;
        let event_digest = self.events_digest;
        let dependencies = vec![];
        TransactionEffects::new_from_execution_v2(
            status,
            executed_epoch,
            GasCostSummary::default(),
            shared_objects,
            self.transaction.digest(),
            lamport_version,
            changed_objects,
            Some(gas_object_id),
            event_digest,
            dependencies,
        )
    }
}
