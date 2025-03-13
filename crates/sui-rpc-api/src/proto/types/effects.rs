use super::TryFromProtoError;
use tap::Pipe;

//
// TransactionEffects
//

impl From<sui_sdk_types::TransactionEffects> for super::TransactionEffects {
    fn from(value: sui_sdk_types::TransactionEffects) -> Self {
        let digest = value.digest();

        let mut message = match value {
            sui_sdk_types::TransactionEffects::V1(v1) => {
                let bcs = super::Bcs::serialize(&sui_sdk_types::TransactionEffects::V1(v1.clone()))
                    .unwrap();
                let mut message = Self::from(*v1);
                message.bcs = Some(bcs);
                message
            }
            sui_sdk_types::TransactionEffects::V2(v2) => Self::from(*v2),
        };

        message.digest = Some(digest.to_string());
        message
    }
}

impl TryFrom<&super::TransactionEffects> for sui_sdk_types::TransactionEffects {
    type Error = TryFromProtoError;

    fn try_from(value: &super::TransactionEffects) -> Result<Self, Self::Error> {
        match value.version() {
            super::Version::Unknown => {
                return Err(TryFromProtoError::from_error(
                    "unknown TransactionEffects version",
                ))
            }
            super::Version::V1 => Self::V1(Box::new(
                sui_sdk_types::TransactionEffectsV1::try_from(value)?,
            )),
            super::Version::V2 => Self::V2(Box::new(
                sui_sdk_types::TransactionEffectsV2::try_from(value)?,
            )),
        }
        .pipe(Ok)
    }
}

//
// TransactionEffectsV1
//

// There isn't a way to correctly encode the ordering needed to round-trip effects V1 through the
// protobuf definition so this is a one-way trip without bcs
impl From<sui_sdk_types::TransactionEffectsV1> for super::TransactionEffects {
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
        use super::ChangedObject;
        use super::UnchangedSharedObject;

        let mut changed_objects = Vec::new();
        let mut unchanged_shared_objects = Vec::new();

        for object in created {
            let (object_id, version, digest) = object.reference.into_parts();
            let owner = object.owner;
            let change = ChangedObject {
                object_id: Some(object_id.to_string()),
                input_state: Some(super::changed_object::InputObjectState::DoesNotExist.into()),
                input_version: None,
                input_digest: None,
                input_owner: None,
                output_state: Some(super::changed_object::OutputObjectState::ObjectWrite.into()),
                output_version: Some(version),
                output_digest: Some(digest.to_string()),
                output_owner: Some(owner.into()),
                id_operation: Some(super::changed_object::IdOperation::Created.into()),
                object_type: None,
            };

            changed_objects.push(change);
        }

        for object in mutated {
            let (object_id, version, digest) = object.reference.into_parts();
            let owner = object.owner;
            let change = ChangedObject {
                object_id: Some(object_id.to_string()),
                input_state: Some(super::changed_object::InputObjectState::Exists.into()),
                input_version: None,
                input_digest: None,
                input_owner: None,
                output_state: Some(super::changed_object::OutputObjectState::ObjectWrite.into()),
                output_version: Some(version),
                output_digest: Some(digest.to_string()),
                output_owner: Some(owner.into()),
                id_operation: Some(super::changed_object::IdOperation::None.into()),
                object_type: None,
            };

            changed_objects.push(change);
        }

        for object in unwrapped {
            let (object_id, version, digest) = object.reference.into_parts();
            let owner = object.owner;
            let change = ChangedObject {
                object_id: Some(object_id.to_string()),
                input_state: Some(super::changed_object::InputObjectState::DoesNotExist.into()),
                input_version: None,
                input_digest: None,
                input_owner: None,
                output_state: Some(super::changed_object::OutputObjectState::ObjectWrite.into()),
                output_version: Some(version),
                output_digest: Some(digest.to_string()),
                output_owner: Some(owner.into()),
                id_operation: Some(super::changed_object::IdOperation::None.into()),
                object_type: None,
            };

            changed_objects.push(change);
        }

        for object in deleted {
            let (object_id, version, digest) = object.into_parts();
            let change = ChangedObject {
                object_id: Some(object_id.to_string()),
                input_state: Some(super::changed_object::InputObjectState::Exists.into()),
                input_version: None,
                input_digest: None,
                input_owner: None,
                output_state: Some(super::changed_object::OutputObjectState::DoesNotExist.into()),
                output_version: Some(version),
                output_digest: Some(digest.to_string()),
                output_owner: None,
                id_operation: Some(super::changed_object::IdOperation::Deleted.into()),
                object_type: None,
            };

            changed_objects.push(change);
        }

        for object in unwrapped_then_deleted {
            let (object_id, version, digest) = object.into_parts();
            let change = ChangedObject {
                object_id: Some(object_id.to_string()),
                input_state: Some(super::changed_object::InputObjectState::DoesNotExist.into()),
                input_version: None,
                input_digest: None,
                input_owner: None,
                output_state: Some(super::changed_object::OutputObjectState::DoesNotExist.into()),
                output_version: Some(version),
                output_digest: Some(digest.to_string()),
                output_owner: None,
                id_operation: Some(super::changed_object::IdOperation::Deleted.into()),
                object_type: None,
            };

            changed_objects.push(change);
        }

        for object in wrapped {
            let (object_id, version, digest) = object.into_parts();
            let change = ChangedObject {
                object_id: Some(object_id.to_string()),
                input_state: Some(super::changed_object::InputObjectState::Exists.into()),
                input_version: None,
                input_digest: None,
                input_owner: None,
                output_state: Some(super::changed_object::OutputObjectState::DoesNotExist.into()),
                output_version: Some(version),
                output_digest: Some(digest.to_string()),
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

        let gas_object_id = gas_object.reference.object_id().to_string();
        let gas_object_index = changed_objects
            .iter()
            .position(|object| object.object_id() == gas_object_id)
            .map(|i| i as u32);

        Self {
            bcs: None,
            digest: None,
            version: Some(super::Version::V1.into()),
            status: Some(status.into()),
            epoch: Some(epoch),
            gas_used: Some(gas_used.into()),
            transaction_digest: Some(transaction_digest.to_string()),
            events_digest: events_digest.map(|d| d.to_string()),
            dependencies: dependencies.iter().map(ToString::to_string).collect(),
            gas_object_index,
            lamport_version: None,
            changed_objects,
            unchanged_shared_objects,
            auxiliary_data_digest: None,
        }
    }
}

// There isn't a way to correctly encode the ordering needed to round-trip effects V1 through the
// protobuf definition so if we want to go this back into an effects v1 struct we have to go
// through bcs
impl TryFrom<&super::TransactionEffects> for sui_sdk_types::TransactionEffectsV1 {
    type Error = TryFromProtoError;

    fn try_from(
        super::TransactionEffects { bcs, version, .. }: &super::TransactionEffects,
    ) -> Result<Self, Self::Error> {
        if *version != Some(super::Version::V1.into()) {
            return Err(TryFromProtoError::from_error(
                "expected TransactionEffects version 1",
            ));
        }

        let effects = bcs
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("bcs"))?
            .deserialize::<sui_sdk_types::TransactionEffects>()
            .map_err(TryFromProtoError::from_error)?;

        if let sui_sdk_types::TransactionEffects::V1(transaction_effects_v1) = effects {
            Ok(*transaction_effects_v1)
        } else {
            Err(TryFromProtoError::from_error(
                "expected TransactionEffects version 1",
            ))
        }
    }
}

//
// TransactionEffectsV2
//

impl From<sui_sdk_types::TransactionEffectsV2> for super::TransactionEffects {
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
            bcs: None,
            digest: None,
            version: Some(super::Version::V2.into()),
            status: Some(status.into()),
            epoch: Some(epoch),
            gas_used: Some(gas_used.into()),
            transaction_digest: Some(transaction_digest.to_string()),
            gas_object_index,
            events_digest: events_digest.map(|d| d.to_string()),
            dependencies: dependencies.iter().map(ToString::to_string).collect(),
            lamport_version: Some(lamport_version),
            changed_objects: changed_objects.into_iter().map(Into::into).collect(),
            unchanged_shared_objects: unchanged_shared_objects
                .into_iter()
                .map(Into::into)
                .collect(),
            auxiliary_data_digest: auxiliary_data_digest.map(|d| d.to_string()),
        }
    }
}

impl TryFrom<&super::TransactionEffects> for sui_sdk_types::TransactionEffectsV2 {
    type Error = TryFromProtoError;

    fn try_from(
        super::TransactionEffects {
            bcs: _,
            digest: _,
            version,
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
        }: &super::TransactionEffects,
    ) -> Result<Self, Self::Error> {
        if *version != Some(super::Version::V2.into()) {
            return Err(TryFromProtoError::from_error(
                "expected TransactionEffects version 2",
            ));
        }

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
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let events_digest = events_digest
            .as_ref()
            .map(|s| s.parse().map_err(TryFromProtoError::from_error))
            .transpose()?;

        let dependencies = dependencies
            .iter()
            .map(|s| s.parse().map_err(TryFromProtoError::from_error))
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
            .map(|s| s.parse().map_err(TryFromProtoError::from_error))
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
            Cancelled { version } => {
                message.version = Some(version);
                UnchangedSharedObjectKind::Cancelled
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
            UnchangedSharedObjectKind::Cancelled => UnchangedSharedKind::Cancelled {
                version: value
                    .version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?,
            },
            UnchangedSharedObjectKind::PerEpochConfig => UnchangedSharedKind::PerEpochConfig,
        };

        Ok(Self { object_id, kind })
    }
}
