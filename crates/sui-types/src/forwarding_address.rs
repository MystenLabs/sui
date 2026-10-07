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
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    dynamic_field::DynamicFieldKey,
    error::SuiResult,
    storage::RuntimeObjectResolver,
};

pub const FORWARDING_ADDRESS_MODULE_NAME: &IdentStr = ident_str!("forwarding_address");
pub const FORWARDING_DEPOSIT_STRUCT_NAME: &IdentStr = ident_str!("ForwardingDeposit");
pub const FORWARDING_TRANSFER_STRUCT_NAME: &IdentStr = ident_str!("ForwardingTransfer");
pub const MASTER_REGISTERED_STRUCT_NAME: &IdentStr = ident_str!("MasterRegistered");
pub const PAUSED_STRUCT_NAME: &IdentStr = ident_str!("Paused");
pub const UNPAUSED_STRUCT_NAME: &IdentStr = ident_str!("Unpaused");
pub const ROTATION_FINALIZED_STRUCT_NAME: &IdentStr = ident_str!("RotationFinalized");
pub const MASTER_RECORD_STRUCT_NAME: &IdentStr = ident_str!("MasterRecord");

/// Layout of a forwarding address:
/// `[u48 master_id LE][FORWARDING_ADDRESS_MAGIC][u8 variant][payload]`.
/// The master id, magic and variant positions are fixed; the variant alone decides what the
/// payload bytes mean. Variant 0 (opaque) gives them no on-chain meaning at all.
pub const FORWARDING_ADDRESS_MAGIC: [u8; 9] = [0xfa; 9];
pub const FORWARDING_ADDRESS_VARIANT_OPAQUE: u8 = 0;
pub const FORWARDING_ADDRESS_PAYLOAD_LENGTH: usize = 16;
pub const FORWARDING_ADDRESS_RESERVED_MASTER_ID: u64 = 0;
/// Master ids are 48 bits wide.
pub const FORWARDING_ADDRESS_MAX_MASTER_ID: u64 = (1 << 48) - 1;

const MASTER_ID_RANGE: std::ops::Range<usize> = 0..6;
const MAGIC_RANGE: std::ops::Range<usize> = 6..15;
const VARIANT_OFFSET: usize = 15;
const PAYLOAD_RANGE: std::ops::Range<usize> = 16..32;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ForwardingAddress {
    pub master_id: u64,
    pub variant: u8,
    pub payload: [u8; FORWARDING_ADDRESS_PAYLOAD_LENGTH],
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ForwardingDeposit {
    pub forwarding_address: SuiAddress,
    pub master: SuiAddress,
    pub amount: u64,
}

impl ForwardingDeposit {
    /// `sui::forwarding_address::ForwardingDeposit<coin_type>`.
    pub fn struct_tag(coin_type: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: FORWARDING_ADDRESS_MODULE_NAME.to_owned(),
            name: FORWARDING_DEPOSIT_STRUCT_NAME.to_owned(),
            type_params: vec![coin_type],
        }
    }
}

/// An object sent to a forwarding address, rerouted to the master.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ForwardingTransfer {
    pub forwarding_address: SuiAddress,
    pub master: SuiAddress,
    pub object_id: ObjectID,
}

impl ForwardingTransfer {
    pub fn struct_tag() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: FORWARDING_ADDRESS_MODULE_NAME.to_owned(),
            name: FORWARDING_TRANSFER_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct MasterRegistered {
    pub master_id: u64,
    pub master: SuiAddress,
    pub cap_id: ObjectID,
    pub rotation_delay_epochs: u64,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RotationFinalized {
    pub master_id: u64,
    pub master: SuiAddress,
}

/// Mirrors `sui::forwarding_address::MasterRecord`, the value of the registry's dynamic field
/// keyed by master id. A Move `Option` serializes like a Rust `Option` for zero or one element.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct MasterRecord {
    pub master: SuiAddress,
    pub paused: bool,
    pub pending: Option<PendingRotation>,
    pub rotation_delay_epochs: u64,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone, Copy)]
pub struct PendingRotation {
    pub new_master: SuiAddress,
    pub effective_epoch: u64,
}

/// What resolution needs to know about a registered master id.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ForwardingMaster {
    pub master: SuiAddress,
    pub paused: bool,
}

impl From<MasterRecord> for ForwardingMaster {
    fn from(record: MasterRecord) -> Self {
        Self {
            master: record.master,
            paused: record.paused,
        }
    }
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
pub struct MasterRecordKey(pub u64);

impl MasterRecordKey {
    fn dynamic_field_key(self) -> DynamicFieldKey<ObjectID, u64> {
        DynamicFieldKey(
            SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
            self.0,
            TypeTag::U64,
        )
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
        master_id: u64,
        variant: u8,
        payload: [u8; FORWARDING_ADDRESS_PAYLOAD_LENGTH],
    ) -> SuiAddress {
        assert!(
            master_id <= FORWARDING_ADDRESS_MAX_MASTER_ID,
            "master id {master_id} does not fit in 48 bits"
        );
        let mut bytes = [0; AccountAddress::LENGTH];
        bytes[MASTER_ID_RANGE].copy_from_slice(&master_id.to_le_bytes()[..6]);
        bytes[MAGIC_RANGE].copy_from_slice(&FORWARDING_ADDRESS_MAGIC);
        bytes[VARIANT_OFFSET] = variant;
        bytes[PAYLOAD_RANGE].copy_from_slice(&payload);
        SuiAddress::from_bytes(bytes).unwrap()
    }

    pub fn derive_opaque(
        master_id: u64,
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
            master_id: {
                let mut id = [0; 8];
                id[..6].copy_from_slice(&bytes[MASTER_ID_RANGE]);
                u64::from_le_bytes(id)
            },
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
        let master_id = 0x1020_3040_5060;
        let payload: [u8; FORWARDING_ADDRESS_PAYLOAD_LENGTH] = [
            0x80, 0xff, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
            0x0c, 0xfe,
        ];
        let address = ForwardingAddress::derive(master_id, 7, payload);
        let bytes = address.to_inner();
        assert_eq!(bytes[0..6], [0x60, 0x50, 0x40, 0x30, 0x20, 0x10]);
        assert_eq!(bytes[6..15], [0xfa; 9]);
        assert_eq!(bytes[15], 7);
        assert_eq!(bytes[16..32], payload);
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

        let mut almost = ForwardingAddress::derive_opaque(1, [2; 16]).to_inner();
        almost[14] = 0xfd;
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
    fn master_record_loads_through_the_registry_schema() {
        let key = MasterRecordKey(0x52ca8647179c);
        let record = MasterRecord {
            master: SuiAddress::random_for_testing_only(),
            paused: true,
            pending: Some(PendingRotation {
                new_master: SuiAddress::random_for_testing_only(),
                effective_epoch: 7,
            }),
            rotation_delay_epochs: 2,
        };
        let field = key.dynamic_field_key().into_field(record.clone()).unwrap();
        let move_object = field
            .into_move_object_unsafe_for_testing(SequenceNumber::from_u64(3))
            .unwrap();
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
        assert_eq!(key.load(&store, registry_version).unwrap(), Some(record));
        assert_eq!(
            MasterRecordKey(1).load(&store, registry_version).unwrap(),
            None
        );
    }
}
