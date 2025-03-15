use super::TryFromProtoError;
use tap::Pipe;

//
// ObjectReference
//

impl From<sui_sdk_types::ObjectReference> for super::ObjectReference {
    fn from(value: sui_sdk_types::ObjectReference) -> Self {
        let (object_id, version, digest) = value.into_parts();
        Self {
            object_id: Some(object_id.to_string()),
            version: Some(version),
            digest: Some(digest.to_string()),
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
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let version = value
            .version
            .ok_or_else(|| TryFromProtoError::missing("version"))?;

        let digest = value
            .digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("digest"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        Ok(Self::new(object_id, version, digest))
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
            Address(address) => Kind::Address(address.to_string()),
            Object(object) => Kind::Object(object.to_string()),
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
            Address(address) => {
                Self::Address(address.parse().map_err(TryFromProtoError::from_error)?)
            }
            Object(object) => Self::Object(object.parse().map_err(TryFromProtoError::from_error)?),
            Shared(version) => Self::Shared(*version),
            Immutable(()) => Self::Immutable,
        }
        .pipe(Ok)
    }
}

//
// Object
//

impl From<sui_sdk_types::Object> for super::Object {
    fn from(value: sui_sdk_types::Object) -> Self {
        let mut message = Self {
            object_id: Some(value.object_id().to_string()),
            version: Some(value.version()),
            digest: Some(value.digest().to_string()),
            owner: Some(value.owner().to_owned().into()),
            previous_transaction: Some(value.previous_transaction().to_string()),
            storage_rebate: Some(value.storage_rebate()),
            ..Default::default()
        };

        match value.data() {
            sui_sdk_types::ObjectData::Struct(move_struct) => {
                set_struct_fields(&mut message, move_struct);
            }
            sui_sdk_types::ObjectData::Package(move_package) => {
                set_package_fields(&mut message, move_package);
            }
        }

        message
    }
}

fn set_struct_fields(message: &mut super::Object, move_struct: &sui_sdk_types::MoveStruct) {
    message.object_type = Some(move_struct.object_type().to_string());
    message.has_public_transfer = Some(move_struct.has_public_transfer());
    message.contents = Some(move_struct.contents().to_vec().into());
}

fn set_package_fields(message: &mut super::Object, move_package: &sui_sdk_types::MovePackage) {
    message.modules = move_package
        .modules
        .iter()
        .map(|(name, contents)| super::MoveModule {
            name: Some(name.to_string()),
            contents: Some(contents.clone().into()),
        })
        .collect();

    message.type_origin_table = move_package
        .type_origin_table
        .clone()
        .into_iter()
        .map(Into::into)
        .collect();

    message.linkage_table = move_package
        .linkage_table
        .iter()
        .map(
            |(
                original_id,
                sui_sdk_types::UpgradeInfo {
                    upgraded_id,
                    upgraded_version,
                },
            )| {
                super::UpgradeInfo {
                    original_id: Some(original_id.to_string()),
                    upgraded_id: Some(upgraded_id.to_string()),
                    upgraded_version: Some(*upgraded_version),
                }
            },
        )
        .collect();
}

fn try_extract_struct(
    value: &super::Object,
) -> Result<sui_sdk_types::MoveStruct, TryFromProtoError> {
    let version = value
        .version
        .ok_or_else(|| TryFromProtoError::missing("version"))?;

    let object_type = value
        .object_type()
        .parse()
        .map_err(TryFromProtoError::from_error)?;

    let has_public_transfer = value
        .has_public_transfer
        .ok_or_else(|| TryFromProtoError::missing("has_public_transfer"))?;
    let contents = value
        .contents
        .as_ref()
        .ok_or_else(|| TryFromProtoError::missing("contents"))?
        .to_vec();

    sui_sdk_types::MoveStruct::new(object_type, has_public_transfer, version, contents)
        .ok_or_else(|| TryFromProtoError::from_error("contents missing object_id"))
}

fn try_extract_package(
    value: &super::Object,
) -> Result<sui_sdk_types::MovePackage, TryFromProtoError> {
    let version = value
        .version
        .ok_or_else(|| TryFromProtoError::missing("version"))?;
    let id = value
        .object_id
        .as_ref()
        .ok_or_else(|| TryFromProtoError::missing("object_id"))?
        .parse()
        .map_err(TryFromProtoError::from_error)?;

    let modules = value
        .modules
        .iter()
        .map(|module| {
            let name = module
                .name
                .as_ref()
                .ok_or_else(|| TryFromProtoError::missing("name"))?
                .parse()
                .map_err(TryFromProtoError::from_error)?;

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
                .parse()
                .map_err(TryFromProtoError::from_error)?;

            let upgraded_id = upgrade_info
                .upgraded_id
                .as_ref()
                .ok_or_else(|| TryFromProtoError::missing("upgraded_id"))?
                .parse()
                .map_err(TryFromProtoError::from_error)?;
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

    Ok(sui_sdk_types::MovePackage {
        id,
        version,
        modules,
        type_origin_table,
        linkage_table,
    })
}

impl TryFrom<&super::Object> for sui_sdk_types::Object {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Object) -> Result<Self, Self::Error> {
        let owner = value
            .owner
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("owner"))?
            .try_into()?;

        let previous_transaction = value
            .previous_transaction
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("previous_transaction"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let storage_rebate = value
            .storage_rebate
            .ok_or_else(|| TryFromProtoError::missing("storage_rebate"))?;

        let object_data = if value.object_type.is_some() {
            // Struct
            sui_sdk_types::ObjectData::Struct(try_extract_struct(value)?)
        } else {
            // Package
            sui_sdk_types::ObjectData::Package(try_extract_package(value)?)
        };

        Ok(Self::new(
            object_data,
            owner,
            previous_transaction,
            storage_rebate,
        ))
    }
}

//
// TypeOrigin
//

impl From<sui_sdk_types::TypeOrigin> for super::TypeOrigin {
    fn from(value: sui_sdk_types::TypeOrigin) -> Self {
        Self {
            module_name: Some(value.module_name.to_string()),
            struct_name: Some(value.struct_name.to_string()),
            package_id: Some(value.package.to_string()),
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
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let struct_name = value
            .struct_name
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("struct_name"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let package = value
            .package_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("package_id"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

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

impl From<sui_sdk_types::GenesisObject> for super::Object {
    fn from(value: sui_sdk_types::GenesisObject) -> Self {
        let mut message = Self {
            object_id: Some(value.object_id().to_string()),
            version: Some(value.version()),
            owner: Some(value.owner().to_owned().into()),
            ..Default::default()
        };

        match value.data() {
            sui_sdk_types::ObjectData::Struct(move_struct) => {
                set_struct_fields(&mut message, move_struct);
            }
            sui_sdk_types::ObjectData::Package(move_package) => {
                set_package_fields(&mut message, move_package);
            }
        }

        message
    }
}

impl TryFrom<&super::Object> for sui_sdk_types::GenesisObject {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Object) -> Result<Self, Self::Error> {
        let object_data = if value.object_type.is_some() {
            // Struct
            sui_sdk_types::ObjectData::Struct(try_extract_struct(value)?)
        } else {
            // Package
            sui_sdk_types::ObjectData::Package(try_extract_package(value)?)
        };

        let owner = value
            .owner
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("owner"))?
            .try_into()?;

        Ok(Self::new(object_data, owner))
    }
}
