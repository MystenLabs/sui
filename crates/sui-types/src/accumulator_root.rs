// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    balance::Balance,
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    digests::{Digest, TransactionDigest},
    dynamic_field::{
        serialize_dynamic_field, DynamicFieldKey, DynamicFieldObject, Field,
        UnboundedDynamicFieldID, DYNAMIC_FIELD_FIELD_STRUCT_NAME, DYNAMIC_FIELD_MODULE_NAME,
    },
    error::{SuiError, SuiResult},
    object::{MoveObject, Object, Owner},
    storage::{ChildObjectResolver, ObjectStore},
    MoveTypeTagTrait, MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_ADDRESS,
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
    u256::U256,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const ACCUMULATOR_ROOT_MODULE: &IdentStr = ident_str!("accumulator");
pub const ACCUMULATOR_SETTLEMENT_MODULE: &IdentStr = ident_str!("accumulator_settlement");
pub const ACCUMULATOR_SETTLEMENT_EVENT_STREAM_HEAD: &IdentStr = ident_str!("EventStreamHead");
pub const ACCUMULATOR_ROOT_CREATE_FUNC: &IdentStr = ident_str!("create");
pub const ACCUMULATOR_ROOT_SETTLE_U128_FUNC: &IdentStr = ident_str!("settle_u128");
pub const ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC: &IdentStr = ident_str!("settlement_prologue");
pub const ACCUMULATOR_ROOT_SETTLEMENT_SETTLE_EVENTS_FUNC: &IdentStr = ident_str!("settle_events");

const ACCUMULATOR_KEY_TYPE: &IdentStr = ident_str!("Key");
const ACCUMULATOR_U128_TYPE: &IdentStr = ident_str!("U128");

pub fn get_accumulator_root_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Accumulator root object must be shared"),
        }))
}

/// Rust type for the Move type AccumulatorKey used to derive the dynamic field id for the
/// balance account object.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccumulatorKey {
    pub owner: SuiAddress,
}

impl MoveTypeTagTraitGeneric for AccumulatorKey {
    fn get_type_tag(type_params: &[TypeTag]) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: ACCUMULATOR_ROOT_MODULE.to_owned(),
            name: ACCUMULATOR_KEY_TYPE.to_owned(),
            type_params: type_params.to_vec(),
        }))
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum AccumulatorValue {
    U128(U128),
}

#[derive(Default, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct U128 {
    pub value: u128,
}

impl MoveTypeTagTrait for U128 {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_ROOT_MODULE.to_owned(),
            name: ACCUMULATOR_U128_TYPE.to_owned(),
            type_params: vec![],
        }))
    }
}

/// New-type for ObjectIDs that are known to have been properly derived as an Balance accumulator field.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct AccumulatorObjId(ObjectID);

impl AccumulatorObjId {
    pub fn new_unchecked(id: ObjectID) -> Self {
        Self(id)
    }

    pub fn inner(&self) -> &ObjectID {
        &self.0
    }
}

impl AccumulatorValue {
    pub fn get_field_id(owner: SuiAddress, type_: &TypeTag) -> SuiResult<AccumulatorObjId> {
        if !Balance::is_balance_type(type_) {
            return Err(SuiError::TypeError {
                error: "only Balance<T> is supported".to_string(),
            });
        }

        let key = AccumulatorKey { owner };
        Ok(AccumulatorObjId(
            DynamicFieldKey(
                SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                key,
                AccumulatorKey::get_type_tag(&[type_.clone()]),
            )
            .into_unbounded_id()?
            .as_object_id(),
        ))
    }

    pub fn exists(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        owner: SuiAddress,
        type_: &TypeTag,
    ) -> SuiResult<bool> {
        if !Balance::is_balance_type(type_) {
            return Err(SuiError::TypeError {
                error: "only Balance<T> is supported".to_string(),
            });
        }

        let key = AccumulatorKey { owner };
        DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            AccumulatorKey::get_type_tag(&[type_.clone()]),
        )
        .into_id_with_bound(version_bound.unwrap_or(SequenceNumber::MAX))?
        .exists(child_object_resolver)
    }

    pub fn load_latest_by_id<T>(
        object_store: &dyn ObjectStore,
        id: AccumulatorObjId,
    ) -> SuiResult<Option<(T, SequenceNumber)>>
    where
        T: Serialize + DeserializeOwned,
    {
        UnboundedDynamicFieldID::<AccumulatorKey>::new(SUI_ACCUMULATOR_ROOT_OBJECT_ID, id.0)
            .load_object(object_store)
            .map(|o| {
                let version = o.0.version();
                o.load_value::<T>().map(|v| (v, version))
            })
            .transpose()
    }

    pub fn load(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        owner: SuiAddress,
        type_: &TypeTag,
    ) -> SuiResult<Option<Self>> {
        if !Balance::is_balance_type(type_) {
            return Err(SuiError::TypeError {
                error: "only Balance<T> is supported".to_string(),
            });
        }

        let key = AccumulatorKey { owner };
        let key_type_tag = AccumulatorKey::get_type_tag(&[type_.clone()]);

        let Some(value) = DynamicFieldKey(SUI_ACCUMULATOR_ROOT_OBJECT_ID, key, key_type_tag)
            .into_id_with_bound(version_bound.unwrap_or(SequenceNumber::MAX))?
            .load_object(child_object_resolver)?
            .map(|o| o.load_value::<U128>())
            .transpose()?
        else {
            return Ok(None);
        };

        Ok(Some(Self::U128(value)))
    }

    pub fn load_object(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        owner: SuiAddress,
        type_: &TypeTag,
    ) -> SuiResult<Option<Object>> {
        let key = AccumulatorKey { owner };
        let key_type_tag = AccumulatorKey::get_type_tag(&[type_.clone()]);

        Ok(
            DynamicFieldKey(SUI_ACCUMULATOR_ROOT_OBJECT_ID, key, key_type_tag)
                .into_id_with_bound(version_bound.unwrap_or(SequenceNumber::MAX))?
                .load_object(child_object_resolver)?
                .map(|o| o.as_object()),
        )
    }

    pub fn create_for_testing(owner: SuiAddress, type_tag: TypeTag, balance: u64) -> Object {
        let key = AccumulatorKey { owner };
        let value = U128 {
            value: balance as u128,
        };

        let field_key = DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            AccumulatorKey::get_type_tag(&[type_tag.clone()]),
        );
        let field = field_key.into_field(value).unwrap();
        let move_object = field
            .into_move_object_unsafe_for_testing(SequenceNumber::new())
            .unwrap();

        Object::new_move(
            move_object,
            Owner::ObjectOwner(SUI_ACCUMULATOR_ROOT_ADDRESS.into()),
            TransactionDigest::genesis_marker(),
        )
    }
}

impl TryFrom<&MoveObject> for AccumulatorValue {
    type Error = SuiError;
    fn try_from(value: &MoveObject) -> Result<Self, Self::Error> {
        value
            .type_()
            .is_balance_accumulator_field()
            .then(|| {
                value
                    .to_rust::<Field<AccumulatorKey, U128>>()
                    .map(|f| f.value)
            })
            .flatten()
            .map(Self::U128)
            .ok_or_else(|| {
                SuiError::DynamicFieldReadError(format!(
                    "Dynamic field {:?} is not a AccumulatorValue",
                    value.id()
                ))
            })
    }
}

pub fn update_account_balance_for_testing(account_object: &mut Object, balance_change: i128) {
    let current_balance_field = DynamicFieldObject::<AccumulatorKey>::new(account_object.clone())
        .load_field::<U128>()
        .unwrap();

    let current_balance = current_balance_field.value.value;

    assert!(current_balance <= i128::MAX as u128);
    assert!(current_balance as i128 >= balance_change.abs());

    let new_balance = U128 {
        value: (current_balance as i128 + balance_change) as u128,
    };

    let new_field = serialize_dynamic_field(
        &current_balance_field.id,
        &current_balance_field.name,
        new_balance,
    )
    .unwrap();

    let move_object = account_object.data.try_as_move_mut().unwrap();
    move_object.set_contents_unsafe(new_field);
}

/// Check if a StructTag is Field<Key<Balance<T>>, U128>
pub(crate) fn is_balance_accumulator_field(s: &StructTag) -> bool {
    s.address == SUI_FRAMEWORK_ADDRESS
        && s.module.as_ident_str() == DYNAMIC_FIELD_MODULE_NAME
        && s.name.as_ident_str() == DYNAMIC_FIELD_FIELD_STRUCT_NAME
        && s.type_params.len() == 2
        && is_accumulator_key_balance(&s.type_params[0])
        && is_accumulator_u128(&s.type_params[1])
}

/// Check if a TypeTag is Key<Balance<T>>
pub(crate) fn is_accumulator_key_balance(t: &TypeTag) -> bool {
    if let TypeTag::Struct(s) = t {
        s.address == SUI_FRAMEWORK_ADDRESS
            && s.module.as_ident_str() == ACCUMULATOR_ROOT_MODULE
            && s.name.as_ident_str() == ACCUMULATOR_KEY_TYPE
            && s.type_params.len() == 1
            && Balance::is_balance_type(&s.type_params[0])
    } else {
        false
    }
}

/// Check if a TypeTag is U128 from accumulator module
pub(crate) fn is_accumulator_u128(t: &TypeTag) -> bool {
    if let TypeTag::Struct(s) = t {
        s.address == SUI_FRAMEWORK_ADDRESS
            && s.module.as_ident_str() == ACCUMULATOR_ROOT_MODULE
            && s.name.as_ident_str() == ACCUMULATOR_U128_TYPE
            && s.type_params.is_empty()
    } else {
        false
    }
}

/// Extract T from Field<Key<Balance<T>>, U128>
pub(crate) fn extract_balance_type_from_field(s: &StructTag) -> Option<TypeTag> {
    if s.type_params.len() != 2 {
        return None;
    }

    if let TypeTag::Struct(key_struct) = &s.type_params[0] {
        if key_struct.type_params.len() == 1 {
            if let TypeTag::Struct(balance_struct) = &key_struct.type_params[0] {
                if Balance::is_balance(balance_struct) && balance_struct.type_params.len() == 1 {
                    return Some(balance_struct.type_params[0].clone());
                }
            }
        }
    }
    None
}

/// Rust representation of the Move EventStreamHead struct from accumulator_settlement module.
/// This represents the state of an authenticated event stream head stored on-chain.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EventStreamHead {
    /// The MMR (Merkle Mountain Range) digest representing the accumulated events
    pub mmr: Vec<U256>,
    /// The checkpoint sequence number when this stream head was last updated
    pub checkpoint_seq: u64,
    /// The total number of events accumulated in this stream
    pub num_events: u64,
}

impl Default for EventStreamHead {
    fn default() -> Self {
        Self::new()
    }
}

impl EventStreamHead {
    pub fn new() -> Self {
        Self {
            mmr: vec![],
            checkpoint_seq: 0,
            num_events: 0,
        }
    }

    pub fn num_events(&self) -> u64 {
        self.num_events
    }

    pub fn checkpoint_seq(&self) -> u64 {
        self.checkpoint_seq
    }

    pub fn mmr(&self) -> &Vec<U256> {
        &self.mmr
    }
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct EventCommitment {
    pub checkpoint_seq: u64,
    pub transaction_idx: u64,
    pub event_idx: u64,
    pub digest: Digest,
}

impl EventCommitment {
    pub fn new(checkpoint_seq: u64, transaction_idx: u64, event_idx: u64, digest: Digest) -> Self {
        Self {
            checkpoint_seq,
            transaction_idx,
            event_idx,
            digest,
        }
    }
}

impl PartialOrd for EventCommitment {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EventCommitment {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.checkpoint_seq, self.transaction_idx, self.event_idx).cmp(&(
            other.checkpoint_seq,
            other.transaction_idx,
            other.event_idx,
        ))
    }
}

pub fn build_event_merkle_root(events: &[EventCommitment]) -> Digest {
    use fastcrypto::hash::Blake2b256;
    use fastcrypto::merkle::MerkleTree;

    debug_assert!(
        events.windows(2).all(|w| w[0] <= w[1]),
        "Events must be ordered by (checkpoint_seq, transaction_idx, event_idx)"
    );

    let merkle_tree = MerkleTree::<Blake2b256>::build_from_unserialized(events.to_vec())
        .expect("failed to serialize event commitments for merkle root");
    let root_node = merkle_tree.root();
    let root_digest = root_node.bytes();
    Digest::new(root_digest)
}
