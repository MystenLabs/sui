// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{Bcs, Object};
use crate::message::{MessageField, MessageFields, MessageMerge};
use crate::proto::TryFromProtoError;
use tap::Pipe;

//
// Object
//

pub const PACKAGE_TYPE: &str = "package";

impl Object {
    pub const BCS_FIELD: &'static MessageField =
        &MessageField::new("bcs").with_message_fields(super::Bcs::FIELDS);
    pub const OBJECT_ID_FIELD: &'static MessageField = &MessageField::new("object_id");
    pub const VERSION_FIELD: &'static MessageField = &MessageField::new("version");
    pub const DIGEST_FIELD: &'static MessageField = &MessageField::new("digest");
    pub const OWNER_FIELD: &'static MessageField = &MessageField::new("owner");
    pub const OBJECT_TYPE_FIELD: &'static MessageField = &MessageField::new("object_type");
    pub const HAS_PUBLIC_TRANSFER_FIELD: &'static MessageField =
        &MessageField::new("has_public_transfer");
    pub const CONTENTS_FIELD: &'static MessageField = &MessageField::new("contents");
    pub const MODULES_FIELD: &'static MessageField = &MessageField::new("modules");
    pub const TYPE_ORIGIN_TABLE_FIELD: &'static MessageField =
        &MessageField::new("type_origin_table");
    pub const LINKAGE_TABLE_FIELD: &'static MessageField = &MessageField::new("linkage_table");
    pub const PREVIOUS_TRANSACTION_FIELD: &'static MessageField =
        &MessageField::new("previous_transaction");
    pub const STORAGE_REBATE_FIELD: &'static MessageField = &MessageField::new("storage_rebate");
}

impl MessageFields for Object {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::BCS_FIELD,
        Self::OBJECT_ID_FIELD,
        Self::VERSION_FIELD,
        Self::DIGEST_FIELD,
        Self::OWNER_FIELD,
        Self::OBJECT_TYPE_FIELD,
        Self::HAS_PUBLIC_TRANSFER_FIELD,
        Self::CONTENTS_FIELD,
        Self::MODULES_FIELD,
        Self::TYPE_ORIGIN_TABLE_FIELD,
        Self::LINKAGE_TABLE_FIELD,
        Self::PREVIOUS_TRANSACTION_FIELD,
        Self::STORAGE_REBATE_FIELD,
    ];
}

impl From<sui_sdk_types::Object> for Object {
    fn from(value: sui_sdk_types::Object) -> Self {
        let mut message = Self::default();
        message.merge(value, &crate::field_mask::FieldMaskTree::new_wildcard());
        message
    }
}

impl MessageMerge<&Object> for Object {
    fn merge(&mut self, source: &Object, mask: &crate::field_mask::FieldMaskTree) {
        let Object {
            bcs,
            object_id,
            version,
            digest,
            owner,
            object_type,
            has_public_transfer,
            contents,
            modules,
            type_origin_table,
            linkage_table,
            previous_transaction,
            storage_rebate,
        } = source;

        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = bcs.clone();
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = digest.clone();
        }

        if mask.contains(Self::OBJECT_ID_FIELD.name) {
            self.object_id = object_id.clone();
        }

        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = *version;
        }

        if mask.contains(Self::OWNER_FIELD.name) {
            self.owner = owner.clone();
        }

        if mask.contains(Self::PREVIOUS_TRANSACTION_FIELD.name) {
            self.previous_transaction = previous_transaction.clone();
        }

        if mask.contains(Self::STORAGE_REBATE_FIELD.name) {
            self.storage_rebate = *storage_rebate;
        }

        if mask.contains(Self::OBJECT_TYPE_FIELD.name) {
            self.object_type = object_type.clone();
        }

        if mask.contains(Self::HAS_PUBLIC_TRANSFER_FIELD.name) {
            self.has_public_transfer = *has_public_transfer;
        }

        if mask.contains(Self::CONTENTS_FIELD.name) {
            self.contents = contents.clone();
        }

        if mask.contains(Self::MODULES_FIELD.name) {
            self.modules = modules.clone();
        }

        if mask.contains(Self::TYPE_ORIGIN_TABLE_FIELD.name) {
            self.type_origin_table = type_origin_table.clone();
        }

        if mask.contains(Self::LINKAGE_TABLE_FIELD.name) {
            self.linkage_table = linkage_table.clone();
        }
    }
}

impl MessageMerge<sui_sdk_types::Object> for Object {
    fn merge(&mut self, source: sui_sdk_types::Object, mask: &crate::field_mask::FieldMaskTree) {
        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = Some(super::Bcs::serialize(&source).unwrap());
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest().to_string());
        }

        if mask.contains(Self::OBJECT_ID_FIELD.name) {
            self.object_id = Some(source.object_id().to_string());
        }

        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = Some(source.version());
        }

        if mask.contains(Self::OWNER_FIELD.name) {
            self.owner = Some(source.owner().to_owned().into());
        }

        if mask.contains(Self::PREVIOUS_TRANSACTION_FIELD.name) {
            self.previous_transaction = Some(source.previous_transaction().to_string());
        }

        if mask.contains(Self::STORAGE_REBATE_FIELD.name) {
            self.storage_rebate = Some(source.storage_rebate());
        }

        match source.data() {
            sui_sdk_types::ObjectData::Struct(move_struct) => {
                self.merge(move_struct, mask);
            }
            sui_sdk_types::ObjectData::Package(move_package) => {
                self.merge(move_package, mask);
            }
        }
    }
}

impl MessageMerge<&sui_sdk_types::MoveStruct> for Object {
    fn merge(
        &mut self,
        source: &sui_sdk_types::MoveStruct,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::OBJECT_TYPE_FIELD.name) {
            self.object_type = Some(source.object_type().to_string());
        }

        if mask.contains(Self::HAS_PUBLIC_TRANSFER_FIELD.name) {
            self.has_public_transfer = Some(source.has_public_transfer());
        }

        if mask.contains(Self::CONTENTS_FIELD.name) {
            self.contents = Some(Bcs {
                name: Some(source.object_type().to_string()),
                value: Some(source.contents().to_vec().into()),
            });
        }
    }
}

impl MessageMerge<&sui_sdk_types::MovePackage> for Object {
    fn merge(
        &mut self,
        source: &sui_sdk_types::MovePackage,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::OBJECT_TYPE_FIELD.name) {
            self.object_type = Some(PACKAGE_TYPE.to_owned());
        }

        if mask.contains(Self::MODULES_FIELD.name) {
            self.modules = source
                .modules
                .iter()
                .map(|(name, contents)| super::MoveModule {
                    name: Some(name.to_string()),
                    contents: Some(contents.clone().into()),
                })
                .collect();
        }

        if mask.contains(Self::TYPE_ORIGIN_TABLE_FIELD.name) {
            self.type_origin_table = source
                .type_origin_table
                .clone()
                .into_iter()
                .map(Into::into)
                .collect();
        }

        if mask.contains(Self::LINKAGE_TABLE_FIELD.name) {
            self.linkage_table = source
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
    }
}

fn try_extract_struct(value: &Object) -> Result<sui_sdk_types::MoveStruct, TryFromProtoError> {
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
        .value()
        .to_vec();

    sui_sdk_types::MoveStruct::new(object_type, has_public_transfer, version, contents)
        .ok_or_else(|| TryFromProtoError::from_error("contents missing object_id"))
}

fn try_extract_package(value: &Object) -> Result<sui_sdk_types::MovePackage, TryFromProtoError> {
    if value.object_type() != PACKAGE_TYPE {
        return Err(TryFromProtoError::from_error(format!(
            "expected type {}, found {}",
            PACKAGE_TYPE,
            value.object_type()
        )));
    }

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

impl TryFrom<&Object> for sui_sdk_types::Object {
    type Error = TryFromProtoError;

    fn try_from(value: &Object) -> Result<Self, Self::Error> {
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

        let object_data = if value.object_type() == PACKAGE_TYPE {
            // Package
            sui_sdk_types::ObjectData::Package(try_extract_package(value)?)
        } else {
            // Struct
            sui_sdk_types::ObjectData::Struct(try_extract_struct(value)?)
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

impl From<sui_sdk_types::GenesisObject> for Object {
    fn from(value: sui_sdk_types::GenesisObject) -> Self {
        let mut message = Self {
            object_id: Some(value.object_id().to_string()),
            version: Some(value.version()),
            owner: Some(value.owner().to_owned().into()),
            ..Default::default()
        };

        match value.data() {
            sui_sdk_types::ObjectData::Struct(move_struct) => {
                message.merge(
                    move_struct,
                    &crate::field_mask::FieldMaskTree::new_wildcard(),
                );
            }
            sui_sdk_types::ObjectData::Package(move_package) => {
                message.merge(
                    move_package,
                    &crate::field_mask::FieldMaskTree::new_wildcard(),
                );
            }
        }

        message
    }
}

impl TryFrom<&Object> for sui_sdk_types::GenesisObject {
    type Error = TryFromProtoError;

    fn try_from(value: &Object) -> Result<Self, Self::Error> {
        let object_data = if value.object_type() == PACKAGE_TYPE {
            // Package
            sui_sdk_types::ObjectData::Package(try_extract_package(value)?)
        } else {
            // Struct
            sui_sdk_types::ObjectData::Struct(try_extract_struct(value)?)
        };

        let owner = value
            .owner
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("owner"))?
            .try_into()?;

        Ok(Self::new(object_data, owner))
    }
}

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
        use super::owner::OwnerKind;
        use sui_sdk_types::Owner::*;

        let mut message = Self::default();

        let kind = match value {
            Address(address) => {
                message.address = Some(address.to_string());
                OwnerKind::Address
            }
            Object(object) => {
                message.address = Some(object.to_string());
                OwnerKind::Object
            }
            Shared(version) => {
                message.version = Some(version);
                OwnerKind::Shared
            }
            Immutable => OwnerKind::Immutable,
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::Owner> for sui_sdk_types::Owner {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Owner) -> Result<Self, Self::Error> {
        use super::owner::OwnerKind;

        match value.kind() {
            OwnerKind::Unknown => return Err(TryFromProtoError::from_error("unknown OwnerKind")),
            OwnerKind::Address => Self::Address(
                value
                    .address()
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            ),
            OwnerKind::Object => Self::Object(
                value
                    .address()
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            ),
            OwnerKind::Shared => Self::Shared(value.version()),
            OwnerKind::Immutable => Self::Immutable,
        }
        .pipe(Ok)
    }
}

impl super::GetObjectRequest {
    pub const READ_MASK_DEFAULT: &str = "object_id,version,digest";
}

impl super::BatchGetObjectsRequest {
    pub const READ_MASK_DEFAULT: &str = super::GetObjectRequest::READ_MASK_DEFAULT;
}
