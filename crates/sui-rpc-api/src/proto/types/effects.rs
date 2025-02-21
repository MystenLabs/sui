use super::TryFromProtoError;
use tap::Pipe;

//
// TransactionEffects
//

impl From<sui_sdk_types::TransactionEffects> for super::TransactionEffects {
    fn from(value: sui_sdk_types::TransactionEffects) -> Self {
        use super::transaction_effects::Version;
        use sui_sdk_types::TransactionEffects::*;

        let version = match value {
            V1(v1) => Version::V1((*v1).into()),
            V2(v2) => Version::V2((*v2).into()),
        };

        Self {
            version: Some(version),
        }
    }
}

impl TryFrom<&super::TransactionEffects> for sui_sdk_types::TransactionEffects {
    type Error = TryFromProtoError;

    fn try_from(value: &super::TransactionEffects) -> Result<Self, Self::Error> {
        use super::transaction_effects::Version::*;

        match value
            .version
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("version"))?
        {
            V1(v1) => Self::V1(Box::new(v1.try_into()?)),
            V2(v2) => Self::V2(Box::new(v2.try_into()?)),
        }
        .pipe(Ok)
    }
}

//
// TransactionEffectsV1
//

impl From<sui_sdk_types::TransactionEffectsV1> for super::TransactionEffectsV1 {
    fn from(
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
        }: sui_sdk_types::TransactionEffectsV1,
    ) -> Self {
        Self {
            status: Some(status.into()),
            epoch: Some(epoch),
            gas_used: Some(gas_used.into()),
            modified_at_versions: modified_at_versions.into_iter().map(Into::into).collect(),
            shared_objects: shared_objects.into_iter().map(Into::into).collect(),
            transaction_digest: Some(transaction_digest.into()),
            created: created.into_iter().map(Into::into).collect(),
            mutated: mutated.into_iter().map(Into::into).collect(),
            unwrapped: unwrapped.into_iter().map(Into::into).collect(),
            deleted: deleted.into_iter().map(Into::into).collect(),
            unwrapped_then_deleted: unwrapped_then_deleted.into_iter().map(Into::into).collect(),
            wrapped: wrapped.into_iter().map(Into::into).collect(),
            gas_object: Some(gas_object.into()),
            events_digest: events_digest.map(Into::into),
            dependencies: dependencies.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::TransactionEffectsV1> for sui_sdk_types::TransactionEffectsV1 {
    type Error = TryFromProtoError;

    fn try_from(
        super::TransactionEffectsV1 {
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
        }: &super::TransactionEffectsV1,
    ) -> Result<Self, Self::Error> {
        let status = status
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("status"))?
            .try_into()?;

        let epoch = epoch.ok_or_else(|| TryFromProtoError::missing("epoch"))?;

        let gas_used = gas_used
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("gas_used"))?
            .try_into()?;

        let transaction_digest = transaction_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("transaction_digest"))?
            .try_into()?;

        let modified_at_versions = modified_at_versions
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;
        let shared_objects = shared_objects
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;
        let created = created
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let mutated = mutated
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let unwrapped = unwrapped
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let deleted = deleted
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let unwrapped_then_deleted = unwrapped_then_deleted
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let wrapped = wrapped
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let gas_object = gas_object
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("gas_object"))?
            .try_into()?;

        let events_digest = events_digest.as_ref().map(TryInto::try_into).transpose()?;

        let dependencies = dependencies
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self {
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
        })
    }
}

//
// TransactionEffectsV2
//

impl From<sui_sdk_types::TransactionEffectsV2> for super::TransactionEffectsV2 {
    fn from(
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
        }: sui_sdk_types::TransactionEffectsV2,
    ) -> Self {
        Self {
            status: Some(status.into()),
            epoch: Some(epoch),
            gas_used: Some(gas_used.into()),
            transaction_digest: Some(transaction_digest.into()),
            gas_object_index,
            events_digest: events_digest.map(Into::into),
            dependencies: dependencies.into_iter().map(Into::into).collect(),
            lamport_version: Some(lamport_version),
            changed_objects: changed_objects.into_iter().map(Into::into).collect(),
            unchanged_shared_objects: unchanged_shared_objects
                .into_iter()
                .map(Into::into)
                .collect(),
            auxiliary_data_digest: auxiliary_data_digest.map(Into::into),
        }
    }
}

impl TryFrom<&super::TransactionEffectsV2> for sui_sdk_types::TransactionEffectsV2 {
    type Error = TryFromProtoError;

    fn try_from(
        super::TransactionEffectsV2 {
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
        }: &super::TransactionEffectsV2,
    ) -> Result<Self, Self::Error> {
        let status = status
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("status"))?
            .try_into()?;
        let epoch = epoch.ok_or_else(|| TryFromProtoError::missing("epoch"))?;

        let gas_used = gas_used
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("gas_used"))?
            .try_into()?;

        let transaction_digest = transaction_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("transaction_digest"))?
            .try_into()?;

        let events_digest = events_digest.as_ref().map(TryInto::try_into).transpose()?;

        let dependencies = dependencies
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let lamport_version =
            lamport_version.ok_or_else(|| TryFromProtoError::missing("lamport_version"))?;

        let changed_objects = changed_objects
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let unchanged_shared_objects = unchanged_shared_objects
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let auxiliary_data_digest = auxiliary_data_digest
            .as_ref()
            .map(TryInto::try_into)
            .transpose()?;

        Ok(Self {
            status,
            epoch,
            gas_used,
            transaction_digest,
            gas_object_index: *gas_object_index,
            events_digest,
            dependencies,
            lamport_version,
            changed_objects,
            unchanged_shared_objects,
            auxiliary_data_digest,
        })
    }
}

//
// ModifiedAtVersion
//

impl From<sui_sdk_types::ModifiedAtVersion> for super::ModifiedAtVersion {
    fn from(value: sui_sdk_types::ModifiedAtVersion) -> Self {
        Self {
            object_id: Some(value.object_id.into()),
            version: Some(value.version),
        }
    }
}

impl TryFrom<&super::ModifiedAtVersion> for sui_sdk_types::ModifiedAtVersion {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ModifiedAtVersion) -> Result<Self, Self::Error> {
        let object_id = value
            .object_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_id"))?
            .try_into()?;
        let version = value
            .version
            .ok_or_else(|| TryFromProtoError::missing("version"))?;

        Ok(Self { object_id, version })
    }
}

//
// ObjectReferenceWithOwner
//

impl From<sui_sdk_types::ObjectReferenceWithOwner> for super::ObjectReferenceWithOwner {
    fn from(value: sui_sdk_types::ObjectReferenceWithOwner) -> Self {
        Self {
            reference: Some(value.reference.into()),
            owner: Some(value.owner.into()),
        }
    }
}

impl TryFrom<&super::ObjectReferenceWithOwner> for sui_sdk_types::ObjectReferenceWithOwner {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ObjectReferenceWithOwner) -> Result<Self, Self::Error> {
        let reference = value
            .reference
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("reference"))?
            .try_into()?;

        let owner = value
            .owner
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("owner"))?
            .try_into()?;

        Ok(Self { reference, owner })
    }
}

//
// ChangedObject
//

impl From<sui_sdk_types::ChangedObject> for super::ChangedObject {
    fn from(value: sui_sdk_types::ChangedObject) -> Self {
        Self {
            object_id: Some(value.object_id.into()),
            input_state: Some(value.input_state.into()),
            output_state: Some(value.output_state.into()),
            id_operation: Some(value.id_operation.into()),
        }
    }
}

impl TryFrom<&super::ChangedObject> for sui_sdk_types::ChangedObject {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ChangedObject) -> Result<Self, Self::Error> {
        let object_id = value
            .object_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_id"))?
            .try_into()?;

        let input_state = value
            .input_state
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("input_state"))?
            .try_into()?;

        let output_state = value
            .output_state
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("output_state"))?
            .try_into()?;

        let id_operation = value
            .id_operation
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("id_operation"))?
            .try_into()?;

        Ok(Self {
            object_id,
            input_state,
            output_state,
            id_operation,
        })
    }
}

//
// InputState
//

impl From<sui_sdk_types::ObjectIn> for super::changed_object::InputState {
    fn from(value: sui_sdk_types::ObjectIn) -> Self {
        match value {
            sui_sdk_types::ObjectIn::NotExist => Self::NotExist(()),
            sui_sdk_types::ObjectIn::Exist {
                version,
                digest,
                owner,
            } => Self::Exist(super::ObjectExist {
                version: Some(version),
                digest: Some(digest.into()),
                owner: Some(owner.into()),
            }),
        }
    }
}

impl TryFrom<&super::changed_object::InputState> for sui_sdk_types::ObjectIn {
    type Error = TryFromProtoError;

    fn try_from(value: &super::changed_object::InputState) -> Result<Self, Self::Error> {
        use super::changed_object::InputState::*;

        match value {
            NotExist(()) => Self::NotExist,
            Exist(super::ObjectExist {
                version,
                digest,
                owner,
            }) => Self::Exist {
                version: version.ok_or_else(|| TryFromProtoError::missing("version"))?,
                digest: digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .try_into()?,
                owner: owner
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("owner"))?
                    .try_into()?,
            },
        }
        .pipe(Ok)
    }
}

//
// OutputState
//

impl From<sui_sdk_types::ObjectOut> for super::changed_object::OutputState {
    fn from(value: sui_sdk_types::ObjectOut) -> Self {
        use sui_sdk_types::ObjectOut::*;
        match value {
            NotExist => Self::Removed(()),
            ObjectWrite { digest, owner } => Self::ObjectWrite(super::ObjectWrite {
                digest: Some(digest.into()),
                owner: Some(owner.into()),
            }),
            PackageWrite { version, digest } => Self::PackageWrite(super::PackageWrite {
                version: Some(version),
                digest: Some(digest.into()),
            }),
        }
    }
}

impl TryFrom<&super::changed_object::OutputState> for sui_sdk_types::ObjectOut {
    type Error = TryFromProtoError;

    fn try_from(value: &super::changed_object::OutputState) -> Result<Self, Self::Error> {
        use super::changed_object::OutputState::*;

        match value {
            Removed(()) => Self::NotExist,
            ObjectWrite(super::ObjectWrite { digest, owner }) => Self::ObjectWrite {
                digest: digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .try_into()?,

                owner: owner
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("owner"))?
                    .try_into()?,
            },
            PackageWrite(super::PackageWrite { version, digest }) => Self::PackageWrite {
                version: version.ok_or_else(|| TryFromProtoError::missing("version"))?,
                digest: digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .try_into()?,
            },
        }
        .pipe(Ok)
    }
}

//
// IdOperation
//

impl From<sui_sdk_types::IdOperation> for super::changed_object::IdOperation {
    fn from(value: sui_sdk_types::IdOperation) -> Self {
        use sui_sdk_types::IdOperation::*;

        match value {
            None => Self::None(()),
            Created => Self::Created(()),
            Deleted => Self::Deleted(()),
        }
    }
}

impl TryFrom<&super::changed_object::IdOperation> for sui_sdk_types::IdOperation {
    type Error = TryFromProtoError;

    fn try_from(value: &super::changed_object::IdOperation) -> Result<Self, Self::Error> {
        use super::changed_object::IdOperation;

        match value {
            IdOperation::None(()) => Self::None,
            IdOperation::Created(()) => Self::Created,
            IdOperation::Deleted(()) => Self::Deleted,
        }
        .pipe(Ok)
    }
}

//
// UnchangedSharedObject
//

impl From<sui_sdk_types::UnchangedSharedObject> for super::UnchangedSharedObject {
    fn from(value: sui_sdk_types::UnchangedSharedObject) -> Self {
        Self {
            object_id: Some(value.object_id.into()),
            kind: Some(value.kind.into()),
        }
    }
}

impl TryFrom<&super::UnchangedSharedObject> for sui_sdk_types::UnchangedSharedObject {
    type Error = TryFromProtoError;

    fn try_from(value: &super::UnchangedSharedObject) -> Result<Self, Self::Error> {
        let object_id = value
            .object_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_id"))?
            .try_into()?;

        let kind = value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
            .try_into()?;

        Ok(Self { object_id, kind })
    }
}

//
// UnchangedSharedKind
//

impl From<sui_sdk_types::UnchangedSharedKind> for super::unchanged_shared_object::Kind {
    fn from(value: sui_sdk_types::UnchangedSharedKind) -> Self {
        use sui_sdk_types::UnchangedSharedKind::*;

        match value {
            ReadOnlyRoot { version, digest } => Self::ReadOnlyRoot(super::ReadOnlyRoot {
                version: Some(version),
                digest: Some(digest.into()),
            }),
            MutateDeleted { version } => Self::MutateDeleted(version),
            ReadDeleted { version } => Self::ReadDeleted(version),
            Cancelled { version } => Self::Cancelled(version),
            PerEpochConfig => Self::PerEpochConfig(()),
        }
    }
}

impl TryFrom<&super::unchanged_shared_object::Kind> for sui_sdk_types::UnchangedSharedKind {
    type Error = TryFromProtoError;

    fn try_from(value: &super::unchanged_shared_object::Kind) -> Result<Self, Self::Error> {
        use super::unchanged_shared_object::Kind::*;

        match value {
            ReadOnlyRoot(super::ReadOnlyRoot { version, digest }) => Self::ReadOnlyRoot {
                version: version.ok_or_else(|| TryFromProtoError::missing("version"))?,

                digest: digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .try_into()?,
            },
            MutateDeleted(version) => Self::MutateDeleted { version: *version },
            ReadDeleted(version) => Self::ReadDeleted { version: *version },
            Cancelled(version) => Self::Cancelled { version: *version },
            PerEpochConfig(()) => Self::PerEpochConfig,
        }
        .pipe(Ok)
    }
}
