// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::TransactionEffects;
use crate::message::{MessageField, MessageFields, MessageMerge};
use crate::proto::TryFromProtoError;
use tap::Pipe;

//
// TransactionEffects
//

impl TransactionEffects {
    pub const BCS_FIELD: &'static MessageField =
        &MessageField::new("bcs").with_message_fields(super::Bcs::FIELDS);
    pub const DIGEST_FIELD: &'static MessageField = &MessageField::new("digest");
    pub const VERSION_FIELD: &'static MessageField = &MessageField::new("version");
    pub const STATUS_FIELD: &'static MessageField = &MessageField::new("status");
    pub const EPOCH_FIELD: &'static MessageField = &MessageField::new("epoch");
    pub const GAS_USED_FIELD: &'static MessageField = &MessageField::new("gas_used");
    pub const TRANSACTION_DIGEST_FIELD: &'static MessageField =
        &MessageField::new("transaction_digest");
    pub const GAS_OBJECT_FIELD: &'static MessageField = &MessageField::new("gas_object");
    pub const EVENTS_DIGEST_FIELD: &'static MessageField = &MessageField::new("events_digest");
    pub const DEPENDENCIES_FIELD: &'static MessageField = &MessageField::new("dependencies");
    pub const LAMPORT_VERSION_FIELD: &'static MessageField = &MessageField::new("lamport_version");
    pub const CHANGED_OBJECTS_FIELD: &'static MessageField = &MessageField::new("changed_objects");
    pub const UNCHANGED_SHARED_OBJECTS_FIELD: &'static MessageField =
        &MessageField::new("unchanged_shared_objects");
    pub const AUXILIARY_DATA_DIGEST_FIELD: &'static MessageField =
        &MessageField::new("auxiliary_data_digest");
}

impl MessageFields for TransactionEffects {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::BCS_FIELD,
        Self::DIGEST_FIELD,
        Self::VERSION_FIELD,
        Self::STATUS_FIELD,
        Self::EPOCH_FIELD,
        Self::GAS_USED_FIELD,
        Self::TRANSACTION_DIGEST_FIELD,
        Self::GAS_OBJECT_FIELD,
        Self::EVENTS_DIGEST_FIELD,
        Self::DEPENDENCIES_FIELD,
        Self::LAMPORT_VERSION_FIELD,
        Self::CHANGED_OBJECTS_FIELD,
        Self::UNCHANGED_SHARED_OBJECTS_FIELD,
        Self::AUXILIARY_DATA_DIGEST_FIELD,
    ];
}

impl From<sui_sdk_types::TransactionEffects> for TransactionEffects {
    fn from(value: sui_sdk_types::TransactionEffects) -> Self {
        let mut message = Self::default();
        message.merge(&value, &crate::field_mask::FieldMaskTree::new_wildcard());
        message
    }
}

impl MessageMerge<&sui_sdk_types::TransactionEffects> for TransactionEffects {
    fn merge(
        &mut self,
        source: &sui_sdk_types::TransactionEffects,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = Some(super::Bcs::serialize(&source).unwrap());
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest().to_string());
        }

        match source {
            sui_sdk_types::TransactionEffects::V1(v1) => self.merge(v1.as_ref(), mask),
            sui_sdk_types::TransactionEffects::V2(v2) => self.merge(v2.as_ref(), mask),
        }
    }
}

impl MessageMerge<&TransactionEffects> for TransactionEffects {
    fn merge(
        &mut self,
        TransactionEffects {
            bcs,
            digest,
            version,
            status,
            epoch,
            gas_used,
            transaction_digest,
            gas_object,
            events_digest,
            dependencies,
            lamport_version,
            changed_objects,
            unchanged_shared_objects,
            auxiliary_data_digest,
        }: &TransactionEffects,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = bcs.clone();
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = digest.clone();
        }
        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = *version;
        }

        if mask.contains(Self::STATUS_FIELD.name) {
            self.status = status.clone();
        }

        if mask.contains(Self::EPOCH_FIELD.name) {
            self.epoch = *epoch;
        }

        if mask.contains(Self::GAS_USED_FIELD.name) {
            self.gas_used = *gas_used;
        }

        if mask.contains(Self::TRANSACTION_DIGEST_FIELD.name) {
            self.transaction_digest = transaction_digest.clone();
        }

        if mask.contains(Self::GAS_OBJECT_FIELD.name) {
            self.gas_object = gas_object.clone();
        }

        if mask.contains(Self::EVENTS_DIGEST_FIELD.name) {
            self.events_digest = events_digest.clone();
        }

        if mask.contains(Self::DEPENDENCIES_FIELD.name) {
            self.dependencies = dependencies.clone();
        }

        if mask.contains(Self::LAMPORT_VERSION_FIELD.name) {
            self.lamport_version = *lamport_version;
        }

        if mask.contains(Self::CHANGED_OBJECTS_FIELD.name) {
            self.changed_objects = changed_objects.clone();
        }

        if mask.contains(Self::UNCHANGED_SHARED_OBJECTS_FIELD.name) {
            self.unchanged_shared_objects = unchanged_shared_objects.clone();
        }

        if mask.contains(Self::AUXILIARY_DATA_DIGEST_FIELD.name) {
            self.auxiliary_data_digest = auxiliary_data_digest.clone();
        }
    }
}

impl TryFrom<&TransactionEffects> for sui_sdk_types::TransactionEffects {
    type Error = TryFromProtoError;

    fn try_from(value: &TransactionEffects) -> Result<Self, Self::Error> {
        value
            .bcs
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("bcs"))?
            .deserialize()
            .map_err(TryFromProtoError::from_error)
    }
}

//
// TransactionEffectsV1
//

impl MessageMerge<&sui_sdk_types::TransactionEffectsV1> for TransactionEffects {
    fn merge(
        &mut self,
        sui_sdk_types::TransactionEffectsV1 {
            status,
            epoch,
            gas_used,
            modified_at_versions,
            shared_objects,
            transaction_digest,
            created,
            mutated,
            unwrapped,
            deleted,
            unwrapped_then_deleted,
            wrapped,
            gas_object,
            events_digest,
            dependencies,
        }: &sui_sdk_types::TransactionEffectsV1,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        use super::ChangedObject;
        use super::UnchangedSharedObject;

        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = Some(1);
        }

        if mask.contains(Self::STATUS_FIELD.name) {
            self.status = Some(status.clone().into());
        }

        if mask.contains(Self::EPOCH_FIELD.name) {
            self.epoch = Some(*epoch);
        }

        if mask.contains(Self::GAS_USED_FIELD.name) {
            self.gas_used = Some(gas_used.clone().into());
        }

        if mask.contains(Self::TRANSACTION_DIGEST_FIELD.name) {
            self.transaction_digest = Some(transaction_digest.to_string());
        }

        if mask.contains(Self::EVENTS_DIGEST_FIELD.name) {
            self.events_digest = events_digest.map(|d| d.to_string());
        }

        if mask.contains(Self::DEPENDENCIES_FIELD.name) {
            self.dependencies = dependencies.iter().map(ToString::to_string).collect();
        }

        if mask.contains(Self::CHANGED_OBJECTS_FIELD.name)
            || mask.contains(Self::UNCHANGED_SHARED_OBJECTS_FIELD.name)
            || mask.contains(Self::GAS_OBJECT_FIELD.name)
        {
            let mut changed_objects = Vec::new();
            let mut unchanged_shared_objects = Vec::new();

            for object in created {
                let change = ChangedObject {
                    object_id: Some(object.reference.object_id().to_string()),
                    input_state: Some(super::changed_object::InputObjectState::DoesNotExist.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(
                        super::changed_object::OutputObjectState::ObjectWrite.into(),
                    ),
                    output_version: Some(object.reference.version()),
                    output_digest: Some(object.reference.digest().to_string()),
                    output_owner: Some(object.owner.into()),
                    id_operation: Some(super::changed_object::IdOperation::Created.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for object in mutated {
                let change = ChangedObject {
                    object_id: Some(object.reference.object_id().to_string()),
                    input_state: Some(super::changed_object::InputObjectState::Exists.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(
                        super::changed_object::OutputObjectState::ObjectWrite.into(),
                    ),
                    output_version: Some(object.reference.version()),
                    output_digest: Some(object.reference.digest().to_string()),
                    output_owner: Some(object.owner.into()),
                    id_operation: Some(super::changed_object::IdOperation::None.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for object in unwrapped {
                let change = ChangedObject {
                    object_id: Some(object.reference.object_id().to_string()),
                    input_state: Some(super::changed_object::InputObjectState::DoesNotExist.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(
                        super::changed_object::OutputObjectState::ObjectWrite.into(),
                    ),
                    output_version: Some(object.reference.version()),
                    output_digest: Some(object.reference.digest().to_string()),
                    output_owner: Some(object.owner.into()),
                    id_operation: Some(super::changed_object::IdOperation::None.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for object in deleted {
                let change = ChangedObject {
                    object_id: Some(object.object_id().to_string()),
                    input_state: Some(super::changed_object::InputObjectState::Exists.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(
                        super::changed_object::OutputObjectState::DoesNotExist.into(),
                    ),
                    output_version: Some(object.version()),
                    output_digest: Some(object.digest().to_string()),
                    output_owner: None,
                    id_operation: Some(super::changed_object::IdOperation::Deleted.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for object in unwrapped_then_deleted {
                let change = ChangedObject {
                    object_id: Some(object.object_id().to_string()),
                    input_state: Some(super::changed_object::InputObjectState::DoesNotExist.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(
                        super::changed_object::OutputObjectState::DoesNotExist.into(),
                    ),
                    output_version: Some(object.version()),
                    output_digest: Some(object.digest().to_string()),
                    output_owner: None,
                    id_operation: Some(super::changed_object::IdOperation::Deleted.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for object in wrapped {
                let change = ChangedObject {
                    object_id: Some(object.object_id().to_string()),
                    input_state: Some(super::changed_object::InputObjectState::Exists.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(
                        super::changed_object::OutputObjectState::DoesNotExist.into(),
                    ),
                    output_version: Some(object.version()),
                    output_digest: Some(object.digest().to_string()),
                    output_owner: None,
                    id_operation: Some(super::changed_object::IdOperation::Deleted.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for modified_at_version in modified_at_versions {
                let object_id = modified_at_version.object_id.to_string();
                let version = modified_at_version.version;
                if let Some(changed_object) = changed_objects
                    .iter_mut()
                    .find(|object| object.object_id() == object_id)
                {
                    changed_object.input_version = Some(version);
                }
            }

            for object in shared_objects {
                let object_id = object.object_id().to_string();
                let version = object.version();
                let digest = object.digest().to_string();

                if let Some(changed_object) = changed_objects
                    .iter_mut()
                    .find(|object| object.object_id() == object_id)
                {
                    changed_object.input_version = Some(version);
                    changed_object.input_digest = Some(digest);
                } else {
                    let unchanged_shared_object = UnchangedSharedObject {
                        kind: Some(
                            super::unchanged_shared_object::UnchangedSharedObjectKind::ReadOnlyRoot
                                .into(),
                        ),
                        object_id: Some(object_id),
                        version: Some(version),
                        digest: Some(digest),
                        object_type: None,
                    };

                    unchanged_shared_objects.push(unchanged_shared_object);
                }
            }

            if mask.contains(Self::GAS_OBJECT_FIELD.name) {
                let gas_object_id = gas_object.reference.object_id().to_string();
                self.gas_object = changed_objects
                    .iter()
                    .find(|object| object.object_id() == gas_object_id)
                    .cloned();
            }

            if mask.contains(Self::CHANGED_OBJECTS_FIELD.name) {
                self.changed_objects = changed_objects;
            }

            if mask.contains(Self::UNCHANGED_SHARED_OBJECTS_FIELD.name) {
                self.unchanged_shared_objects = unchanged_shared_objects;
            }
        }
    }
}

//
// TransactionEffectsV2
//

impl MessageMerge<&sui_sdk_types::TransactionEffectsV2> for TransactionEffects {
    fn merge(
        &mut self,
        sui_sdk_types::TransactionEffectsV2 {
            status,
            epoch,
            gas_used,
            transaction_digest,
            gas_object_index,
            events_digest,
            dependencies,
            lamport_version,
            changed_objects,
            unchanged_shared_objects,
            auxiliary_data_digest,
        }: &sui_sdk_types::TransactionEffectsV2,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = Some(2);
        }

        if mask.contains(Self::STATUS_FIELD.name) {
            self.status = Some(status.clone().into());
        }

        if mask.contains(Self::EPOCH_FIELD.name) {
            self.epoch = Some(*epoch);
        }

        if mask.contains(Self::GAS_USED_FIELD.name) {
            self.gas_used = Some(gas_used.clone().into());
        }

        if mask.contains(Self::TRANSACTION_DIGEST_FIELD.name) {
            self.transaction_digest = Some(transaction_digest.to_string());
        }

        if mask.contains(Self::GAS_OBJECT_FIELD.name) {
            self.gas_object = gas_object_index
                .map(|index| changed_objects.get(index as usize).cloned().map(Into::into))
                .flatten();
        }

        if mask.contains(Self::EVENTS_DIGEST_FIELD.name) {
            self.events_digest = events_digest.map(|d| d.to_string());
        }

        if mask.contains(Self::DEPENDENCIES_FIELD.name) {
            self.dependencies = dependencies.iter().map(ToString::to_string).collect();
        }

        if mask.contains(Self::LAMPORT_VERSION_FIELD.name) {
            self.lamport_version = Some(*lamport_version);
        }

        if mask.contains(Self::CHANGED_OBJECTS_FIELD.name) {
            self.changed_objects = changed_objects
                .clone()
                .into_iter()
                .map(Into::into)
                .collect();
        }

        if mask.contains(Self::UNCHANGED_SHARED_OBJECTS_FIELD.name) {
            self.unchanged_shared_objects = unchanged_shared_objects
                .clone()
                .into_iter()
                .map(Into::into)
                .collect();
        }

        if mask.contains(Self::AUXILIARY_DATA_DIGEST_FIELD.name) {
            self.auxiliary_data_digest = auxiliary_data_digest.map(|d| d.to_string());
        }
    }
}

//
// ChangedObject
//

impl From<sui_sdk_types::ChangedObject> for super::ChangedObject {
    fn from(value: sui_sdk_types::ChangedObject) -> Self {
        use super::changed_object::InputObjectState;
        use super::changed_object::OutputObjectState;

        let mut message = Self {
            object_id: Some(value.object_id.to_string()),
            ..Default::default()
        };

        // Input State
        let input_state = match value.input_state {
            sui_sdk_types::ObjectIn::NotExist => InputObjectState::DoesNotExist,
            sui_sdk_types::ObjectIn::Exist {
                version,
                digest,
                owner,
            } => {
                message.input_version = Some(version);
                message.input_digest = Some(digest.to_string());
                message.input_owner = Some(owner.into());
                InputObjectState::Exists
            }
        };
        message.set_input_state(input_state);

        // Output State
        let output_state = match value.output_state {
            sui_sdk_types::ObjectOut::NotExist => OutputObjectState::DoesNotExist,
            sui_sdk_types::ObjectOut::ObjectWrite { digest, owner } => {
                message.output_digest = Some(digest.to_string());
                message.output_owner = Some(owner.into());
                OutputObjectState::ObjectWrite
            }
            sui_sdk_types::ObjectOut::PackageWrite { version, digest } => {
                message.output_version = Some(version);
                message.output_digest = Some(digest.to_string());
                OutputObjectState::PackageWrite
            }
        };
        message.set_output_state(output_state);

        message.set_id_operation(value.id_operation.into());
        message
    }
}

impl TryFrom<&super::ChangedObject> for sui_sdk_types::ChangedObject {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ChangedObject) -> Result<Self, Self::Error> {
        use super::changed_object::InputObjectState;
        use super::changed_object::OutputObjectState;

        let object_id = value
            .object_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_id"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let input_state = match value.input_state() {
            InputObjectState::Unknown => {
                return Err(TryFromProtoError::from_error("unknown InputObjectState"))
            }
            InputObjectState::DoesNotExist => sui_sdk_types::ObjectIn::NotExist,
            InputObjectState::Exists => sui_sdk_types::ObjectIn::Exist {
                version: value
                    .input_version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?,
                digest: value
                    .input_digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
                owner: value
                    .input_owner
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("owner"))?
                    .try_into()?,
            },
        };

        let output_state = match value.output_state() {
            OutputObjectState::Unknown => {
                return Err(TryFromProtoError::from_error("unknown OutputObjectState"))
            }
            OutputObjectState::DoesNotExist => sui_sdk_types::ObjectOut::NotExist,
            OutputObjectState::ObjectWrite => sui_sdk_types::ObjectOut::ObjectWrite {
                digest: value
                    .output_digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,

                owner: value
                    .output_owner
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("owner"))?
                    .try_into()?,
            },
            OutputObjectState::PackageWrite => sui_sdk_types::ObjectOut::PackageWrite {
                version: value
                    .output_version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?,
                digest: value
                    .output_digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            },
        };

        let id_operation = value.id_operation().try_into()?;

        Ok(Self {
            object_id,
            input_state,
            output_state,
            id_operation,
        })
    }
}

//
// IdOperation
//

impl From<sui_sdk_types::IdOperation> for super::changed_object::IdOperation {
    fn from(value: sui_sdk_types::IdOperation) -> Self {
        use sui_sdk_types::IdOperation::*;

        match value {
            None => Self::None,
            Created => Self::Created,
            Deleted => Self::Deleted,
        }
    }
}

impl TryFrom<super::changed_object::IdOperation> for sui_sdk_types::IdOperation {
    type Error = TryFromProtoError;

    fn try_from(value: super::changed_object::IdOperation) -> Result<Self, Self::Error> {
        use super::changed_object::IdOperation;

        match value {
            IdOperation::Unknown => {
                return Err(TryFromProtoError::from_error("unknown IdOperation"))
            }
            IdOperation::None => Self::None,
            IdOperation::Created => Self::Created,
            IdOperation::Deleted => Self::Deleted,
        }
        .pipe(Ok)
    }
}

//
// UnchangedSharedObject
//

impl From<sui_sdk_types::UnchangedSharedObject> for super::UnchangedSharedObject {
    fn from(value: sui_sdk_types::UnchangedSharedObject) -> Self {
        use super::unchanged_shared_object::UnchangedSharedObjectKind;
        use sui_sdk_types::UnchangedSharedKind::*;

        let mut message = Self {
            object_id: Some(value.object_id.to_string()),
            ..Default::default()
        };

        let kind = match value.kind {
            ReadOnlyRoot { version, digest } => {
                message.version = Some(version);
                message.digest = Some(digest.to_string());
                UnchangedSharedObjectKind::ReadOnlyRoot
            }
            MutateDeleted { version } => {
                message.version = Some(version);
                UnchangedSharedObjectKind::MutateDeleted
            }
            ReadDeleted { version } => {
                message.version = Some(version);
                UnchangedSharedObjectKind::ReadDeleted
            }
            Canceled { version } => {
                message.version = Some(version);
                UnchangedSharedObjectKind::Canceled
            }
            PerEpochConfig => UnchangedSharedObjectKind::PerEpochConfig,
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::UnchangedSharedObject> for sui_sdk_types::UnchangedSharedObject {
    type Error = TryFromProtoError;

    fn try_from(value: &super::UnchangedSharedObject) -> Result<Self, Self::Error> {
        use super::unchanged_shared_object::UnchangedSharedObjectKind;
        use sui_sdk_types::UnchangedSharedKind;

        let object_id = value
            .object_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_id"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let kind = match value.kind() {
            UnchangedSharedObjectKind::Unknown => {
                return Err(TryFromProtoError::from_error("unknown InputKind"))
            }

            UnchangedSharedObjectKind::ReadOnlyRoot => UnchangedSharedKind::ReadOnlyRoot {
                version: value
                    .version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?,

                digest: value
                    .digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            },
            UnchangedSharedObjectKind::MutateDeleted => UnchangedSharedKind::MutateDeleted {
                version: value
                    .version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?,
            },
            UnchangedSharedObjectKind::ReadDeleted => UnchangedSharedKind::ReadDeleted {
                version: value
                    .version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?,
            },
            UnchangedSharedObjectKind::Canceled => UnchangedSharedKind::Canceled {
                version: value
                    .version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?,
            },
            UnchangedSharedObjectKind::PerEpochConfig => UnchangedSharedKind::PerEpochConfig,
        };

        Ok(Self { object_id, kind })
    }
}
