// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    MoveTypeTagTrait, MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_ADDRESS,
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
    accumulator_event::AccumulatorEvent,
    balance::Balance,
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    digests::{Digest, TransactionDigest},
    dynamic_field::{
        BoundedDynamicFieldID, DYNAMIC_FIELD_FIELD_STRUCT_NAME, DYNAMIC_FIELD_MODULE_NAME,
        DynamicFieldKey, DynamicFieldObject, Field, serialize_dynamic_field,
    },
    error::{SuiError, SuiErrorKind, SuiResult},
    object::{MoveObject, Object, Owner},
    storage::{ChildObjectResolver, ObjectStore},
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
    u256::U256,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const ACCUMULATOR_ROOT_MODULE: &IdentStr = ident_str!("accumulator");
pub const ACCUMULATOR_METADATA_MODULE: &IdentStr = ident_str!("accumulator_metadata");
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

/// Rust type for the Move type accumulator::Key used to derive the dynamic field id for the
/// accumulator value.
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

/// New-type for ObjectIDs that are known to have been properly derived as a Balance accumulator field.
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

impl std::fmt::Display for AccumulatorObjId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AccumulatorValue {
    pub fn as_u128(&self) -> Option<u128> {
        match self {
            AccumulatorValue::U128(value) => Some(value.value),
        }
    }

    pub fn get_field_id(owner: SuiAddress, type_: &TypeTag) -> SuiResult<AccumulatorObjId> {
        if !Balance::is_balance_type(type_) {
            return Err(SuiErrorKind::TypeError {
                error: "only Balance<T> is supported".to_string(),
            }
            .into());
        }

        let key = AccumulatorKey { owner };
        Ok(AccumulatorObjId(
            DynamicFieldKey(
                SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                key,
                AccumulatorKey::get_type_tag(std::slice::from_ref(type_)),
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
            return Err(SuiErrorKind::TypeError {
                error: "only Balance<T> is supported".to_string(),
            }
            .into());
        }

        let key = AccumulatorKey { owner };
        DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            AccumulatorKey::get_type_tag(std::slice::from_ref(type_)),
        )
        .into_id_with_bound(version_bound.unwrap_or(SequenceNumber::MAX))?
        .exists(child_object_resolver)
    }

    pub fn load_by_id<T>(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        id: AccumulatorObjId,
    ) -> SuiResult<Option<T>>
    where
        T: Serialize + DeserializeOwned,
    {
        BoundedDynamicFieldID::<AccumulatorKey>::new(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            id.0,
            version_bound.unwrap_or(SequenceNumber::MAX),
        )
        .load_object(child_object_resolver)?
        .map(|o| o.load_value::<T>())
        .transpose()
    }

    pub fn load(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        owner: SuiAddress,
        type_: &TypeTag,
    ) -> SuiResult<Option<Self>> {
        if !Balance::is_balance_type(type_) {
            return Err(SuiErrorKind::TypeError {
                error: "only Balance<T> is supported".to_string(),
            }
            .into());
        }

        let key = AccumulatorKey { owner };
        let key_type_tag = AccumulatorKey::get_type_tag(std::slice::from_ref(type_));

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
        let key_type_tag = AccumulatorKey::get_type_tag(std::slice::from_ref(type_));

        Ok(
            DynamicFieldKey(SUI_ACCUMULATOR_ROOT_OBJECT_ID, key, key_type_tag)
                .into_id_with_bound(version_bound.unwrap_or(SequenceNumber::MAX))?
                .load_object(child_object_resolver)?
                .map(|o| o.into_object()),
        )
    }

    pub fn load_object_by_id(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        id: ObjectID,
    ) -> SuiResult<Option<Object>> {
        Ok(BoundedDynamicFieldID::<AccumulatorKey>::new(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            id,
            version_bound.unwrap_or(SequenceNumber::MAX),
        )
        .load_object(child_object_resolver)?
        .map(|o| o.into_object()))
    }

    #[deprecated(note = "Use try_create_for_testing and handle errors explicitly")]
    pub fn create_for_testing(owner: SuiAddress, type_tag: TypeTag, balance: u64) -> Object {
        Self::try_create_for_testing(owner, type_tag, balance)
            .expect("Failed to create accumulator value for testing")
    }

    pub fn try_create_for_testing(
        owner: SuiAddress,
        type_tag: TypeTag,
        balance: u64,
    ) -> SuiResult<Object> {
        let key = AccumulatorKey { owner };
        let value = U128 {
            value: balance as u128,
        };

        let field_key = DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            AccumulatorKey::get_type_tag(std::slice::from_ref(&type_tag)),
        );
        let field = field_key.into_field(value)?;
        let move_object = field.into_move_object_unsafe_for_testing(SequenceNumber::new())?;

        Ok(Object::new_move(
            move_object,
            Owner::ObjectOwner(SUI_ACCUMULATOR_ROOT_ADDRESS.into()),
            TransactionDigest::genesis_marker(),
        ))
    }
}

/// Extract stream id from an accumulator event if it targets sui::accumulator_settlement::EventStreamHead
pub fn stream_id_from_accumulator_event(ev: &AccumulatorEvent) -> Option<SuiAddress> {
    if let TypeTag::Struct(tag) = &ev.write.address.ty
        && tag.address == SUI_FRAMEWORK_ADDRESS
        && tag.module.as_ident_str() == ACCUMULATOR_SETTLEMENT_MODULE
        && tag.name.as_ident_str() == ACCUMULATOR_SETTLEMENT_EVENT_STREAM_HEAD
    {
        return Some(ev.write.address.address);
    }
    None
}

impl TryFrom<&MoveObject> for AccumulatorValue {
    type Error = SuiError;
    fn try_from(value: &MoveObject) -> Result<Self, Self::Error> {
        let (_key, value): (AccumulatorKey, AccumulatorValue) = value.try_into()?;
        Ok(value)
    }
}

impl TryFrom<&MoveObject> for (AccumulatorKey, AccumulatorValue) {
    type Error = SuiError;
    fn try_from(value: &MoveObject) -> Result<Self, Self::Error> {
        value
            .type_()
            .is_balance_accumulator_field()
            .then(|| value.to_rust::<Field<AccumulatorKey, U128>>())
            .flatten()
            .map(|f| (f.name, AccumulatorValue::U128(f.value)))
            .ok_or_else(|| {
                SuiErrorKind::DynamicFieldReadError(format!(
                    "Dynamic field {:?} is not a AccumulatorValue",
                    value.id()
                ))
                .into()
            })
    }
}

#[deprecated(note = "Use try_update_account_balance_for_testing and handle errors explicitly")]
pub fn update_account_balance_for_testing(account_object: &mut Object, balance_change: i128) {
    try_update_account_balance_for_testing(account_object, balance_change)
        .expect("Failed to update account balance for testing")
}

pub fn try_update_account_balance_for_testing(
    account_object: &mut Object,
    balance_change: i128,
) -> SuiResult<()> {
    let current_balance_field =
        DynamicFieldObject::<AccumulatorKey>::new(account_object.clone()).load_field::<U128>()?;

    let current_balance = current_balance_field.value.value;

    if current_balance > i128::MAX as u128 {
        return Err(SuiErrorKind::TypeError {
            error: format!("Balance {} exceeds i128::MAX", current_balance),
        }
        .into());
    }

    if (current_balance as i128) < balance_change.abs() {
        return Err(SuiErrorKind::TypeError {
            error: format!(
                "Insufficient balance {} for change {}",
                current_balance, balance_change
            ),
        }
        .into());
    }

    let new_balance = U128 {
        value: (current_balance as i128 + balance_change) as u128,
    };

    let new_field = serialize_dynamic_field(
        &current_balance_field.id,
        &current_balance_field.name,
        new_balance,
    )?;

    let move_object =
        account_object
            .data
            .try_as_move_mut()
            .ok_or_else(|| SuiErrorKind::TypeError {
                error: "Object is not a Move object".to_string(),
            })?;
    move_object.set_contents_unsafe(new_field);
    Ok(())
}

pub(crate) fn accumulator_value_balance_type_maybe(s: &StructTag) -> Option<TypeTag> {
    if s.address == SUI_FRAMEWORK_ADDRESS
        && s.module.as_ident_str() == DYNAMIC_FIELD_MODULE_NAME
        && s.name.as_ident_str() == DYNAMIC_FIELD_FIELD_STRUCT_NAME
        && s.type_params.len() == 2
        && let Some(key_type) = accumulator_key_type_maybe(&s.type_params[0])
        && is_accumulator_u128(&s.type_params[1])
    {
        Balance::maybe_get_balance_type_param(&key_type)
    } else {
        None
    }
}

/// Check if a TypeTag is Key<Balance<T>>
pub(crate) fn accumulator_key_type_maybe(t: &TypeTag) -> Option<TypeTag> {
    if let TypeTag::Struct(s) = t
        && s.address == SUI_FRAMEWORK_ADDRESS
        && s.module.as_ident_str() == ACCUMULATOR_ROOT_MODULE
        && s.name.as_ident_str() == ACCUMULATOR_KEY_TYPE
        && s.type_params.len() == 1
    {
        Some(s.type_params[0].clone())
    } else {
        None
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

pub fn derive_event_stream_head_object_id(stream_id: SuiAddress) -> SuiResult<ObjectID> {
    let key = AccumulatorKey { owner: stream_id };

    let value_type_tag = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ACCUMULATOR_SETTLEMENT_MODULE.to_owned(),
        name: ACCUMULATOR_SETTLEMENT_EVENT_STREAM_HEAD.to_owned(),
        type_params: vec![],
    }));

    let key_type_tag = AccumulatorKey::get_type_tag(&[value_type_tag]);

    DynamicFieldKey(SUI_ACCUMULATOR_ROOT_OBJECT_ID, key, key_type_tag)
        .into_unbounded_id()
        .map(|id| id.as_object_id())
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

#[deprecated(note = "Use try_build_event_merkle_root and handle errors explicitly")]
pub fn build_event_merkle_root(events: &[EventCommitment]) -> Digest {
    try_build_event_merkle_root(events)
        .expect("failed to serialize event commitments for merkle root")
}

pub fn try_build_event_merkle_root(events: &[EventCommitment]) -> SuiResult<Digest> {
    use fastcrypto::hash::Blake2b256;
    use fastcrypto::merkle::MerkleTree;

    debug_assert!(
        events.windows(2).all(|w| w[0] <= w[1]),
        "Events must be ordered by (checkpoint_seq, transaction_idx, event_idx)"
    );

    let merkle_tree =
        MerkleTree::<Blake2b256>::build_from_unserialized(events.to_vec()).map_err(|e| {
            SuiErrorKind::GenericAuthorityError {
                error: format!(
                    "Failed to serialize event commitments for merkle root: {}",
                    e
                ),
            }
        })?;
    let root_node = merkle_tree.root();
    let root_digest = root_node.bytes();
    Ok(Digest::new(root_digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas_coin::GAS;

    #[test]
    fn test_build_event_merkle_root_success() {
        let events = vec![
            EventCommitment::new(1, 0, 0, Digest::random()),
            EventCommitment::new(1, 0, 1, Digest::random()),
            EventCommitment::new(2, 0, 0, Digest::random()),
        ];

        let result = try_build_event_merkle_root(&events);
        assert!(result.is_ok());
        let digest = result.unwrap();
        let bytes: &[u8] = digest.as_ref();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_build_event_merkle_root_empty() {
        let events = vec![];
        let result = try_build_event_merkle_root(&events);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_event_merkle_root_single_event() {
        let events = vec![EventCommitment::new(1, 0, 0, Digest::random())];
        let result = try_build_event_merkle_root(&events);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_for_testing_success() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let balance = 1000u64;

        let result = AccumulatorValue::try_create_for_testing(owner, type_tag, balance);
        assert!(result.is_ok());

        let obj = result.unwrap();
        assert!(obj.is_child_object());
        assert_eq!(
            obj.owner,
            Owner::ObjectOwner(SUI_ACCUMULATOR_ROOT_ADDRESS.into())
        );
    }

    #[test]
    fn test_create_for_testing_zero_balance() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let balance = 0u64;

        let result = AccumulatorValue::try_create_for_testing(owner, type_tag, balance);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_for_testing_max_balance() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let balance = u64::MAX;

        let result = AccumulatorValue::try_create_for_testing(owner, type_tag, balance);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_balance_success_positive_change() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let initial_balance = 1000u64;

        let mut obj =
            AccumulatorValue::try_create_for_testing(owner, type_tag, initial_balance).unwrap();
        let result = try_update_account_balance_for_testing(&mut obj, 500);
        assert!(result.is_ok());

        let move_obj = obj.data.try_as_move().unwrap();
        let field: Field<AccumulatorKey, U128> = move_obj.to_rust().unwrap();
        assert_eq!(field.value.value, 1500);
    }

    #[test]
    fn test_update_balance_success_negative_change() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let initial_balance = 1000u64;

        let mut obj =
            AccumulatorValue::try_create_for_testing(owner, type_tag, initial_balance).unwrap();
        let result = try_update_account_balance_for_testing(&mut obj, -500);
        assert!(result.is_ok());

        let move_obj = obj.data.try_as_move().unwrap();
        let field: Field<AccumulatorKey, U128> = move_obj.to_rust().unwrap();
        assert_eq!(field.value.value, 500);
    }

    #[test]
    fn test_update_balance_success_zero_change() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let initial_balance = 1000u64;

        let mut obj =
            AccumulatorValue::try_create_for_testing(owner, type_tag, initial_balance).unwrap();
        let result = try_update_account_balance_for_testing(&mut obj, 0);
        assert!(result.is_ok());

        let move_obj = obj.data.try_as_move().unwrap();
        let field: Field<AccumulatorKey, U128> = move_obj.to_rust().unwrap();
        assert_eq!(field.value.value, 1000);
    }

    #[test]
    fn test_update_balance_insufficient_balance_error() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let initial_balance = 500u64;

        let mut obj =
            AccumulatorValue::try_create_for_testing(owner, type_tag, initial_balance).unwrap();
        let result = try_update_account_balance_for_testing(&mut obj, -1000);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(*err, SuiErrorKind::TypeError { .. }));
        assert!(err.to_string().contains("Insufficient balance"));
    }

    #[test]
    fn test_update_balance_exact_withdrawal() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let initial_balance = 1000u64;

        let mut obj =
            AccumulatorValue::try_create_for_testing(owner, type_tag, initial_balance).unwrap();
        let result = try_update_account_balance_for_testing(&mut obj, -1000);
        assert!(result.is_ok());

        let move_obj = obj.data.try_as_move().unwrap();
        let field: Field<AccumulatorKey, U128> = move_obj.to_rust().unwrap();
        assert_eq!(field.value.value, 0);
    }

    #[test]
    fn test_update_balance_to_zero() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let initial_balance = 100u64;

        let mut obj =
            AccumulatorValue::try_create_for_testing(owner, type_tag, initial_balance).unwrap();
        let result = try_update_account_balance_for_testing(&mut obj, -100);

        assert!(result.is_ok());
        let move_obj = obj.data.try_as_move().unwrap();
        let field: Field<AccumulatorKey, U128> = move_obj.to_rust().unwrap();
        assert_eq!(field.value.value, 0);
    }

    #[test]
    fn test_event_commitment_ordering() {
        let event1 = EventCommitment::new(1, 0, 0, Digest::random());
        let event2 = EventCommitment::new(1, 0, 1, Digest::random());
        let event3 = EventCommitment::new(1, 1, 0, Digest::random());
        let event4 = EventCommitment::new(2, 0, 0, Digest::random());

        assert!(event1 < event2);
        assert!(event2 < event3);
        assert!(event3 < event4);
    }

    #[test]
    fn test_accumulator_value_as_u128() {
        let value = AccumulatorValue::U128(U128 { value: 12345 });
        assert_eq!(value.as_u128(), Some(12345));
    }

    #[test]
    fn test_event_stream_head_default() {
        let head = EventStreamHead::default();
        assert_eq!(head.num_events(), 0);
        assert_eq!(head.checkpoint_seq(), 0);
        assert!(head.mmr().is_empty());
    }

    #[test]
    fn test_derive_event_stream_head_object_id() {
        let stream_id = SuiAddress::random_for_testing_only();
        let result = derive_event_stream_head_object_id(stream_id);
        assert!(result.is_ok());
    }

    #[test]
    #[allow(deprecated)]
    fn test_legacy_api_signatures() {
        let owner = SuiAddress::random_for_testing_only();
        let type_tag = GAS::type_tag();
        let events = vec![EventCommitment::new(1, 0, 0, Digest::random())];

        let mut obj: Object = AccumulatorValue::create_for_testing(owner, type_tag, 100);
        let _: () = update_account_balance_for_testing(&mut obj, 1);
        let _digest: Digest = build_event_merkle_root(&events);
    }
}
