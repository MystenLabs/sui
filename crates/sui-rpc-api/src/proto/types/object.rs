use super::TryFromProtoError;
use tap::Pipe;

//
// ObjectReference
//

impl From<sui_sdk_types::ObjectReference> for super::ObjectReference {
    fn from(value: sui_sdk_types::ObjectReference) -> Self {
        let (object_id, version, digest) = value.into_parts();
        Self {
            object_id: Some(object_id.into()),
            version: Some(version),
            digest: Some(digest.into()),
        }
    }
}

impl TryFrom<&super::ObjectReference> for sui_sdk_types::ObjectReference {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ObjectReference) -> Result<Self, Self::Error> {
        let object_id = value
            .object_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_id"))?
            .try_into()?;

        let version = value
            .version
            .ok_or_else(|| TryFromProtoError::missing("version"))?;

        let digest = value
            .digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("digest"))?
            .try_into()?;

        Ok(Self::new(object_id, version, digest))
    }
}

//
// Object
//

impl From<sui_sdk_types::Object> for super::Object {
    fn from(value: sui_sdk_types::Object) -> Self {
        Self {
            object_id: Some(value.object_id().into()),
            version: Some(value.version()),
            owner: Some(value.owner().to_owned().into()),
            object: Some(value.data().to_owned().into()),
            previous_transaction: Some(value.previous_transaction().into()),
            storage_rebate: Some(value.storage_rebate()),
        }
    }
}

impl TryFrom<&super::Object> for sui_sdk_types::Object {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Object) -> Result<Self, Self::Error> {
        let owner = value
            .owner
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("owner"))?
            .try_into()?;
        let object_data = value
            .object
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_data"))?
            .try_into()?;

        let previous_transaction = value
            .previous_transaction
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("previous_transaction"))?
            .try_into()?;
        let storage_rebate = value
            .storage_rebate
            .ok_or_else(|| TryFromProtoError::missing("storage_rebate"))?;

        Ok(Self::new(
            object_data,
            owner,
            previous_transaction,
            storage_rebate,
        ))
    }
}

//
// Owner
//

impl From<sui_sdk_types::Owner> for super::Owner {
    fn from(value: sui_sdk_types::Owner) -> Self {
        use super::owner::Kind;
        use sui_sdk_types::Owner::*;

        let kind = match value {
            Address(address) => Kind::Address(address.into()),
            Object(object) => Kind::Object(object.into()),
            Shared(version) => Kind::Shared(version),
            Immutable => Kind::Immutable(()),
        };

        Self { kind: Some(kind) }
    }
}

impl TryFrom<&super::Owner> for sui_sdk_types::Owner {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Owner) -> Result<Self, Self::Error> {
        use super::owner::Kind::*;

        match value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
        {
            Address(address) => Self::Address(address.try_into()?),
            Object(object) => Self::Object(object.try_into()?),
            Shared(version) => Self::Shared(*version),
            Immutable(()) => Self::Immutable,
        }
        .pipe(Ok)
    }
}

//
// ObjectData
//

impl From<sui_sdk_types::ObjectData> for super::ObjectData {
    fn from(value: sui_sdk_types::ObjectData) -> Self {
        use super::object_data::Kind;
        use sui_sdk_types::ObjectData::*;

        let kind = match value {
            Struct(s) => Kind::Struct(s.into()),
            Package(p) => Kind::Package(p.into()),
        };

        Self { kind: Some(kind) }
    }
}

impl TryFrom<&super::ObjectData> for sui_sdk_types::ObjectData {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ObjectData) -> Result<Self, Self::Error> {
        use super::object_data::Kind::*;

        match value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
        {
            Struct(s) => Self::Struct(s.try_into()?),
            Package(p) => Self::Package(p.try_into()?),
        }
        .pipe(Ok)
    }
}

//
// MoveStruct
//

impl From<sui_sdk_types::MoveStruct> for super::MoveStruct {
    fn from(value: sui_sdk_types::MoveStruct) -> Self {
        Self {
            object_id: Some(value.object_id().into()),
            object_type: Some(value.object_type().to_owned().into()),
            has_public_transfer: Some(value.has_public_transfer()),
            version: Some(value.version()),
            contents: Some(value.contents().to_vec().into()),
        }
    }
}

impl TryFrom<&super::MoveStruct> for sui_sdk_types::MoveStruct {
    type Error = TryFromProtoError;

    fn try_from(
        super::MoveStruct {
            object_id: _,
            object_type,
            has_public_transfer,
            version,
            contents,
        }: &super::MoveStruct,
    ) -> Result<Self, Self::Error> {
        let object_type = object_type
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_type"))?
            .try_into()?;

        let has_public_transfer =
            has_public_transfer.ok_or_else(|| TryFromProtoError::missing("has_public_transfer"))?;
        let version = version.ok_or_else(|| TryFromProtoError::missing("version"))?;
        let contents = contents
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("contents"))?
            .to_vec();

        Self::new(object_type, has_public_transfer, version, contents)
            .ok_or_else(|| TryFromProtoError::from_error("contents missing object_id"))
    }
}

//
// MovePackage
//

impl From<sui_sdk_types::MovePackage> for super::MovePackage {
    fn from(value: sui_sdk_types::MovePackage) -> Self {
        let modules = value
            .modules
            .into_iter()
            .map(|(name, contents)| super::MoveModule {
                name: Some(name.into()),
                contents: Some(contents.into()),
            })
            .collect();

        let type_origin_table = value
            .type_origin_table
            .into_iter()
            .map(Into::into)
            .collect();

        let linkage_table = value
            .linkage_table
            .into_iter()
            .map(
                |(
                    original_id,
                    sui_sdk_types::UpgradeInfo {
                        upgraded_id,
                        upgraded_version,
                    },
                )| {
                    super::UpgradeInfo {
                        original_id: Some(original_id.into()),
                        upgraded_id: Some(upgraded_id.into()),
                        upgraded_version: Some(upgraded_version),
                    }
                },
            )
            .collect();

        Self {
            id: Some(value.id.into()),
            version: Some(value.version),
            modules,
            type_origin_table,
            linkage_table,
        }
    }
}

impl TryFrom<&super::MovePackage> for sui_sdk_types::MovePackage {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MovePackage) -> Result<Self, Self::Error> {
        let id = value
            .id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("id"))?
            .try_into()?;

        let modules = value
            .modules
            .iter()
            .map(|module| {
                let name = module
                    .name
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("name"))?
                    .try_into()?;

                let contents = module
                    .contents
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("contents"))?
                    .to_vec();

                Ok((name, contents))
            })
            .collect::<Result<_, TryFromProtoError>>()?;

        let type_origin_table = value
            .type_origin_table
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let linkage_table = value
            .linkage_table
            .iter()
            .map(|upgrade_info| {
                let original_id = upgrade_info
                    .original_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("original_id"))?
                    .try_into()?;

                let upgraded_id = upgrade_info
                    .upgraded_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("upgraded_id"))?
                    .try_into()?;
                let upgraded_version = upgrade_info
                    .upgraded_version
                    .ok_or_else(|| TryFromProtoError::missing("upgraded_version"))?;

                Ok((
                    original_id,
                    sui_sdk_types::UpgradeInfo {
                        upgraded_id,
                        upgraded_version,
                    },
                ))
            })
            .collect::<Result<_, TryFromProtoError>>()?;

        let version = value
            .version
            .ok_or_else(|| TryFromProtoError::missing("version"))?;

        Ok(Self {
            id,
            version,
            modules,
            type_origin_table,
            linkage_table,
        })
    }
}

//
// TypeOrigin
//

impl From<sui_sdk_types::TypeOrigin> for super::TypeOrigin {
    fn from(value: sui_sdk_types::TypeOrigin) -> Self {
        Self {
            module_name: Some(value.module_name.into()),
            struct_name: Some(value.struct_name.into()),
            package_id: Some(value.package.into()),
        }
    }
}

impl TryFrom<&super::TypeOrigin> for sui_sdk_types::TypeOrigin {
    type Error = TryFromProtoError;

    fn try_from(value: &super::TypeOrigin) -> Result<Self, Self::Error> {
        let module_name = value
            .module_name
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("module_name"))?
            .try_into()?;

        let struct_name = value
            .struct_name
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("struct_name"))?
            .try_into()?;

        let package = value
            .package_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("package_id"))?
            .try_into()?;

        Ok(Self {
            module_name,
            struct_name,
            package,
        })
    }
}

//
// GenesisObject
//

impl From<sui_sdk_types::GenesisObject> for super::GenesisObject {
    fn from(value: sui_sdk_types::GenesisObject) -> Self {
        Self {
            object_id: Some(value.object_id().into()),
            version: Some(value.version()),
            owner: Some(value.owner().to_owned().into()),
            object: Some(value.data().to_owned().into()),
        }
    }
}

impl TryFrom<&super::GenesisObject> for sui_sdk_types::GenesisObject {
    type Error = TryFromProtoError;

    fn try_from(value: &super::GenesisObject) -> Result<Self, Self::Error> {
        let object_data = value
            .object
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_data"))?
            .try_into()?;

        let owner = value
            .owner
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("owner"))?
            .try_into()?;

        Ok(Self::new(object_data, owner))
    }
}
