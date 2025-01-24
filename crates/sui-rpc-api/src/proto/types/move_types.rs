use super::TryFromProtoError;
use tap::Pipe;

//
// Identifier
//

impl From<sui_sdk_types::Identifier> for super::Identifier {
    fn from(value: sui_sdk_types::Identifier) -> Self {
        Self {
            identifier: Some(value.into_inner().into()),
        }
    }
}

impl TryFrom<&super::Identifier> for sui_sdk_types::Identifier {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Identifier) -> Result<Self, Self::Error> {
        value
            .identifier
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("identifier"))?
            .pipe(Self::new)
            .map_err(TryFromProtoError::from_error)
    }
}

//
// StructTag
//

impl From<sui_sdk_types::StructTag> for super::StructTag {
    fn from(value: sui_sdk_types::StructTag) -> Self {
        Self {
            address: Some(value.address.into()),
            module: Some(value.module.into()),
            name: Some(value.name.into()),
            type_parameters: value.type_params.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::StructTag> for sui_sdk_types::StructTag {
    type Error = TryFromProtoError;

    fn try_from(value: &super::StructTag) -> Result<Self, Self::Error> {
        let address = value
            .address
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("address"))?
            .pipe(TryFrom::try_from)?;
        let module = value
            .module
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("module"))?
            .pipe(TryFrom::try_from)?;
        let name = value
            .name
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("name"))?
            .pipe(TryFrom::try_from)?;
        let type_params = value
            .type_parameters
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            address,
            module,
            name,
            type_params,
        })
    }
}

//
// TypeTag
//

impl From<sui_sdk_types::TypeTag> for super::TypeTag {
    fn from(value: sui_sdk_types::TypeTag) -> Self {
        use super::type_tag::Tag;
        use sui_sdk_types::TypeTag;

        let tag = match value {
            TypeTag::U8 => Tag::U8(()),
            TypeTag::U16 => Tag::U16(()),
            TypeTag::U32 => Tag::U32(()),
            TypeTag::U64 => Tag::U64(()),
            TypeTag::U128 => Tag::U128(()),
            TypeTag::U256 => Tag::U256(()),
            TypeTag::Bool => Tag::Bool(()),
            TypeTag::Address => Tag::Address(()),
            TypeTag::Signer => Tag::Signer(()),
            TypeTag::Vector(type_tag) => Tag::Vector(Box::new((*type_tag).into())),
            TypeTag::Struct(struct_tag) => Tag::Struct((*struct_tag).into()),
        };

        Self { tag: Some(tag) }
    }
}

impl TryFrom<&super::TypeTag> for sui_sdk_types::TypeTag {
    type Error = TryFromProtoError;

    fn try_from(value: &super::TypeTag) -> Result<Self, Self::Error> {
        use super::type_tag::Tag;

        match value
            .tag
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("type_tag"))?
        {
            Tag::U8(()) => Self::U8,
            Tag::U16(()) => Self::U16,
            Tag::U32(()) => Self::U32,
            Tag::U64(()) => Self::U64,
            Tag::U128(()) => Self::U128,
            Tag::U256(()) => Self::U256,
            Tag::Bool(()) => Self::Bool,
            Tag::Address(()) => Self::Address,
            Tag::Signer(()) => Self::Signer,
            Tag::Vector(type_tag) => Self::Vector(Box::new(type_tag.as_ref().try_into()?)),
            Tag::Struct(struct_tag) => Self::Struct(Box::new(struct_tag.try_into()?)),
        }
        .pipe(Ok)
    }
}
