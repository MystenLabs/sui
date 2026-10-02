// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

use crate::{
    MoveTypeTagTrait, SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_ADDRESS,
    base_types::{MoveObjectType, ObjectID, SequenceNumber, SuiAddress},
    dynamic_field::{DynamicFieldInfo, DynamicFieldKey, Field},
    error::{SuiErrorKind, SuiResult},
    storage::RuntimeObjectResolver,
};

pub const FORWARDING_ADDRESS_MODULE_NAME: &IdentStr = ident_str!("forwarding_address");
pub const FORWARDING_DEPOSIT_STRUCT_NAME: &IdentStr = ident_str!("ForwardingDeposit");
pub const MASTER_REGISTERED_STRUCT_NAME: &IdentStr = ident_str!("MasterRegistered");
pub const MASTER_RECORD_STRUCT_NAME: &IdentStr = ident_str!("MasterRecord");

/// Layout of a forwarding address:
/// `[u32 master_id LE][FORWARDING_ADDRESS_MAGIC][u8 variant][payload]`.
/// The master id, magic and variant positions are fixed; the variant alone decides what the
/// payload bytes mean. Variant 0 (opaque) gives them no on-chain meaning at all.
pub const FORWARDING_ADDRESS_MAGIC: [u8; 10] = [0xfa; 10];
pub const FORWARDING_ADDRESS_VARIANT_OPAQUE: u8 = 0;
pub const FORWARDING_ADDRESS_PAYLOAD_LENGTH: usize = 17;
pub const FORWARDING_ADDRESS_RESERVED_MASTER_ID: u32 = 0;

const MASTER_ID_RANGE: std::ops::Range<usize> = 0..4;
const MAGIC_RANGE: std::ops::Range<usize> = 4..14;
const VARIANT_OFFSET: usize = 14;
const PAYLOAD_RANGE: std::ops::Range<usize> = 15..32;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ForwardingAddress {
    pub master_id: u32,
    pub variant: u8,
    pub payload: [u8; FORWARDING_ADDRESS_PAYLOAD_LENGTH],
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ForwardingDeposit {
    pub forwarding_address: SuiAddress,
    pub master: SuiAddress,
    pub amount: u64,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct MasterRegistered {
    pub master_id: u32,
    pub master: SuiAddress,
    pub cap_id: ObjectID,
}

/// Mirrors `sui::forwarding_address::MasterRecord`, the value of the registry's dynamic field
/// keyed by master id.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct MasterRecord {
    pub master: SuiAddress,
}

impl MoveTypeTagTrait for MasterRecord {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: FORWARDING_ADDRESS_MODULE_NAME.to_owned(),
            name: MASTER_RECORD_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }))
    }
}

/// The registry dynamic field `master_id -> MasterRecord`: where it lives and how to read it.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MasterRecordKey(pub u32);

impl MasterRecordKey {
    fn dynamic_field_key(self) -> DynamicFieldKey<ObjectID, u32> {
        DynamicFieldKey(
            SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
            self.0,
            TypeTag::U32,
        )
    }

    pub fn object_id(self) -> SuiResult<ObjectID> {
        self.dynamic_field_key().object_id()
    }

    /// Move type of the field object, `Field<u32, MasterRecord>`.
    pub fn object_type() -> MoveObjectType {
        DynamicFieldInfo::dynamic_field_type(TypeTag::U32, MasterRecord::get_type_tag()).into()
    }

    /// Decodes the BCS contents of the field object.
    pub fn decode(self, contents: &[u8]) -> SuiResult<MasterRecord> {
        let field: Field<u32, MasterRecord> = bcs::from_bytes(contents)
            .map_err(|err| SuiErrorKind::DynamicFieldReadError(err.to_string()))?;
        if field.name != self.0 {
            return Err(SuiErrorKind::DynamicFieldReadError(format!(
                "master record for id {} is keyed by {}",
                self.0, field.name
            ))
            .into());
        }
        Ok(field.value)
    }

    /// Reads the record as of `registry_version`; `None` if the id is unregistered.
    pub fn load(
        self,
        resolver: &dyn RuntimeObjectResolver,
        registry_version: SequenceNumber,
    ) -> SuiResult<Option<MasterRecord>> {
        self.dynamic_field_key()
            .into_id_with_bound(registry_version)?
            .load_object(resolver)?
            .map(|object| object.load_value::<MasterRecord>())
            .transpose()
    }
}

impl ForwardingAddress {
    pub fn derive(
        master_id: u32,
        variant: u8,
        payload: [u8; FORWARDING_ADDRESS_PAYLOAD_LENGTH],
    ) -> SuiAddress {
        let mut bytes = [0; AccountAddress::LENGTH];
        bytes[MASTER_ID_RANGE].copy_from_slice(&master_id.to_le_bytes());
        bytes[MAGIC_RANGE].copy_from_slice(&FORWARDING_ADDRESS_MAGIC);
        bytes[VARIANT_OFFSET] = variant;
        bytes[PAYLOAD_RANGE].copy_from_slice(&payload);
        SuiAddress::from_bytes(bytes).unwrap()
    }

    pub fn derive_opaque(
        master_id: u32,
        payload: [u8; FORWARDING_ADDRESS_PAYLOAD_LENGTH],
    ) -> SuiAddress {
        Self::derive(master_id, FORWARDING_ADDRESS_VARIANT_OPAQUE, payload)
    }

    /// Whether the address carries the forwarding magic. Everything that must refuse to treat
    /// such an address as an ordinary recipient keys off this.
    pub fn has_magic(address: SuiAddress) -> bool {
        address.to_inner()[MAGIC_RANGE] == FORWARDING_ADDRESS_MAGIC
    }

    /// `None` is an ordinary address. Every address with the magic parses; whether its variant is
    /// supported is a protocol decision made by the caller.
    pub fn parse(address: SuiAddress) -> Option<Self> {
        let bytes = address.to_inner();
        if bytes[MAGIC_RANGE] != FORWARDING_ADDRESS_MAGIC {
            return None;
        }
        Some(Self {
            master_id: u32::from_le_bytes(bytes[MASTER_ID_RANGE].try_into().unwrap()),
            variant: bytes[VARIANT_OFFSET],
            payload: bytes[PAYLOAD_RANGE].try_into().unwrap(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{in_memory_storage::InMemoryStorage, object::Object};

    fn raw(bytes: [u8; AccountAddress::LENGTH]) -> SuiAddress {
        SuiAddress::from_bytes(bytes).unwrap()
    }

    #[test]
    fn round_trips_every_payload_byte() {
        let master_id = 0x1020_3040;
        let payload: [u8; FORWARDING_ADDRESS_PAYLOAD_LENGTH] = [
            0x80, 0xff, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
            0x0c, 0x0d, 0xfe,
        ];
        let address = ForwardingAddress::derive(master_id, 7, payload);
        let bytes = address.to_inner();
        assert_eq!(bytes[0..4], [0x40, 0x30, 0x20, 0x10]);
        assert_eq!(bytes[4..14], [0xfa; 10]);
        assert_eq!(bytes[14], 7);
        assert_eq!(bytes[15..32], payload);
        assert_eq!(
            ForwardingAddress::parse(address),
            Some(ForwardingAddress {
                master_id,
                variant: 7,
                payload,
            })
        );
        assert!(ForwardingAddress::has_magic(address));
    }

    #[test]
    fn ordinary_addresses_have_no_magic() {
        let ordinary = raw([0x42; AccountAddress::LENGTH]);
        assert_eq!(ForwardingAddress::parse(ordinary), None);
        assert!(!ForwardingAddress::has_magic(ordinary));

        let mut almost = ForwardingAddress::derive_opaque(1, [2; 17]).to_inner();
        almost[13] = 0xfd;
        assert_eq!(ForwardingAddress::parse(raw(almost)), None);
        assert!(!ForwardingAddress::has_magic(raw(almost)));

        for id in [
            SuiAddress::ZERO,
            crate::SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID.into(),
            crate::SUI_FRAMEWORK_ADDRESS.into(),
        ] {
            assert_eq!(ForwardingAddress::parse(id), None);
        }
    }

    #[test]
    fn master_record_field_round_trips_through_the_registry_schema() {
        let key = MasterRecordKey(0x688990c0);
        let master = SuiAddress::random_for_testing_only();
        let field = key
            .dynamic_field_key()
            .into_field(MasterRecord { master })
            .unwrap();
        let move_object = field
            .into_move_object_unsafe_for_testing(SequenceNumber::from_u64(3))
            .unwrap();
        assert_eq!(move_object.id(), key.object_id().unwrap());
        assert_eq!(move_object.type_(), &MasterRecordKey::object_type());
        assert_eq!(
            key.decode(move_object.contents()).unwrap(),
            MasterRecord { master }
        );
        assert!(MasterRecordKey(1).decode(move_object.contents()).is_err());

        let registry_version = SequenceNumber::from_u64(5);
        let registry = Object::with_id_owner_version_for_testing(
            SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
            registry_version,
            crate::object::Owner::Shared {
                initial_shared_version: SequenceNumber::from_u64(1),
            },
        );
        let field_object = Object::new_move(
            move_object,
            crate::object::Owner::ObjectOwner(SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID.into()),
            crate::digests::TransactionDigest::genesis_marker(),
        );
        let store = InMemoryStorage::new(vec![registry, field_object]);
        assert_eq!(
            key.load(&store, registry_version).unwrap(),
            Some(MasterRecord { master })
        );
        assert_eq!(
            MasterRecordKey(1).load(&store, registry_version).unwrap(),
            None
        );
    }
}
