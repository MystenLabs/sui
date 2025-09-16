// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::static_programmable_transactions::{env::Env, typing::ast::Type};
use move_binary_format::errors::PartialVMError;
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::execution::interpreter::locals::{BaseHeap as VMBaseHeap, BaseHeapId};
use move_vm_runtime::shared::views::ValueVisitor;
use move_vm_runtime::{
    execution::values::{self, Struct, VMValueCast, Value as VMValue, VectorSpecialization},
    shared::views::ValueView,
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
    error::ExecutionError,
    move_package::{UpgradeCap, UpgradeReceipt, UpgradeTicket},
};
pub enum InputValue<'a> {
    Bytes(&'a ByteValue),
    Loaded(Local<'a>),
}

pub enum ByteValue {
    Pure(Vec<u8>),
    Receiving {
        id: ObjectID,
        version: SequenceNumber,
    },
}

/// A memory location that can be borrowed or moved from
pub struct Local<'a>(&'a mut Locals, u16);

/// A set of memory locations that can be borrowed or moved from. Used for inputs and results
pub struct Locals {
    heap: VMBaseHeap,
    locations: BTreeMap<u16, BaseHeapId>,
}

#[derive(Debug)]
pub struct Value(VMValue);

impl Locals {
    pub fn new<Items>(values: Items) -> Result<Self, ExecutionError>
    where
        Items: IntoIterator<Item = Option<Value>>,
        Items::IntoIter: ExactSizeIterator,
    {
        let values = values.into_iter();
        let n = values.len();
        assert_invariant!(n <= u16::MAX as usize, "Locals size exceeds u16::MAX");
        // TODO(vm-rewrite): Look into not allocation invalid memory slots ahead of time. For now
        // we do this for ease, but we should be able to optimize this further.
        let mut heap = VMBaseHeap::new();
        let mut locations = BTreeMap::new();
        for (i, v) in values.enumerate() {
            let alloc_idx = match v {
                Some(v) => heap.allocate_value(v.0),
                // If the value is None, we leave the local invalid
                None => heap.allocate_value(VMValue::invalid()),
            };
            locations.insert(i as u16, alloc_idx);
        }
        Ok(Self { heap, locations })
    }

    pub fn new_invalid(n: usize) -> Result<Self, ExecutionError> {
        assert_invariant!(n <= u16::MAX as usize, "Locals size exceeds u16::MAX");
        let mut heap = VMBaseHeap::new();
        let mut locations = BTreeMap::new();
        for i in 0..n {
            let alloc_idx = heap.allocate_value(VMValue::invalid());
            locations.insert(i as u16, alloc_idx);
        }
        Ok(Self { heap, locations })
    }

    pub fn local(&mut self, index: u16) -> Result<Local, ExecutionError> {
        Ok(Local(self, index))
    }
}

impl Local<'_> {
    fn to_resolved_location(&self) -> Result<BaseHeapId, ExecutionError> {
        self.0
            .locations
            .get(&self.1)
            .copied()
            .ok_or_else(|| make_invariant_violation!("local index {} out of bounds", self.1))
    }

    /// Does the local contain a value?
    pub fn is_invalid(&self) -> Result<bool, ExecutionError> {
        self.0
            .heap
            .is_invalid(self.to_resolved_location()?)
            .map_err(iv("out of bounds"))
    }

    pub fn store(&mut self, value: Value) -> Result<(), ExecutionError> {
        let val: values::Reference = self
            .0
            .heap
            .borrow_loc(self.to_resolved_location()?)
            .map_err(iv("store loc"))?
            .cast()
            .map_err(iv("cast to reference"))?;
        val.write_ref(value.0).map_err(iv("store loc"))?;
        Ok(())
    }

    /// Move the value out of the local
    pub fn move_(&mut self) -> Result<Value, ExecutionError> {
        assert_invariant!(!self.is_invalid()?, "cannot move invalid local");
        self.0
            .heap
            .take_loc(self.to_resolved_location()?)
            .map_err(iv("move loc"))
            .map(Value)
    }

    /// Copy the value out in the local
    pub fn copy(&self) -> Result<Value, ExecutionError> {
        assert_invariant!(!self.is_invalid()?, "cannot copy invalid local");
        let val: values::Reference = self
            .0
            .heap
            .borrow_loc(self.to_resolved_location()?)
            .map_err(iv("copy loc"))?
            .cast()
            .map_err(iv("cast to reference"))?;
        val.read_ref().map_err(iv("copy loc")).map(Value)
    }

    /// Borrow the local, creating a reference to the value
    pub fn borrow(&mut self) -> Result<Value, ExecutionError> {
        assert_invariant!(!self.is_invalid()?, "cannot borrow invalid local");
        self.0
            .heap
            .borrow_loc(self.to_resolved_location()?)
            .map_err(iv("borrow loc"))
            .map(Value)
    }

    pub fn move_if_valid(&mut self) -> Result<Option<Value>, ExecutionError> {
        if self.is_invalid()? {
            Ok(None)
        } else {
            Ok(Some(self.move_()?))
        }
    }
}

impl Value {
    pub fn copy(&self) -> Result<Self, ExecutionError> {
        Ok(Value(self.0.copy_value()))
    }

    /// Read the value, giving an invariant violation if the value is not a reference
    pub fn read_ref(self) -> Result<Self, ExecutionError> {
        let value: values::Reference = self.0.cast().map_err(iv("cast"))?;
        Ok(Self(value.read_ref().map_err(iv("read ref"))?))
    }

    /// This function will invariant violation on an invalid cast
    pub fn cast<V>(self) -> Result<V, ExecutionError>
    where
        VMValue: VMValueCast<V>,
    {
        self.0.cast().map_err(iv("cast"))
    }

    pub fn deserialize(env: &Env, bytes: &[u8], ty: Type) -> Result<Value, ExecutionError> {
        let layout = env.runtime_layout(&ty)?;
        let Some(value) = VMValue::simple_deserialize(bytes, &layout) else {
            // we already checked the layout of pure bytes during typing
            // and objects should already be valid
            invariant_violation!("unable to deserialize value to type {ty:?}")
        };
        Ok(Value(value))
    }

    pub fn serialize(&self) -> Option<Vec<u8>> {
        self.0.serialize()
    }
}

impl From<VMValue> for Value {
    fn from(value: VMValue) -> Self {
        Value(value)
    }
}

impl From<Value> for VMValue {
    fn from(value: Value) -> Self {
        value.0
    }
}

impl VMValueCast<Value> for VMValue {
    fn cast(self) -> Result<Value, PartialVMError> {
        Ok(self.into())
    }
}

impl ValueView for Value {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        self.0.visit(visitor)
    }
}

//**************************************************************************************************
// Value Construction
//**************************************************************************************************

impl Value {
    pub fn id(address: AccountAddress) -> Self {
        // ID { address }
        Self(VMValue::struct_(Struct::pack([VMValue::address(address)])))
    }

    pub fn uid(address: AccountAddress) -> Self {
        // UID { ID { address } }
        Self(VMValue::struct_(Struct::pack([Self::id(address).0])))
    }

    pub fn receiving(id: ObjectID, version: SequenceNumber) -> Self {
        Self(VMValue::struct_(Struct::pack([
            Self::id(id.into()).0,
            VMValue::u64(version.into()),
        ])))
    }

    pub fn balance(amount: u64) -> Self {
        // Balance { amount }
        Self(VMValue::struct_(Struct::pack([VMValue::u64(amount)])))
    }

    /// The uid _must_ be registered by the object runtime before being called
    pub fn coin(id: ObjectID, amount: u64) -> Self {
        Self(VMValue::struct_(Struct::pack([
            Self::uid(id.into()).0,
            Self::balance(amount).0,
        ])))
    }

    pub fn vec_pack(ty: Type, values: Vec<Self>) -> Result<Self, ExecutionError> {
        let specialization: VectorSpecialization = ty
            .try_into()
            .map_err(|e| make_invariant_violation!("Unable to specialize vector: {e}"))?;
        let vec = values::Vector::pack(specialization, values.into_iter().map(|v| v.0))
            .map_err(iv("pack"))?;
        Ok(Self(vec))
    }

    /// Should be called once at the start of a transaction to populate the location with the
    /// transaction context.
    pub fn new_tx_context(digest: TransactionDigest) -> Result<Self, ExecutionError> {
        // public struct TxContext has drop {
        //     sender: address,
        //     tx_hash: vector<u8>,
        //     epoch: u64,
        //     epoch_timestamp_ms: u64,
        //     ids_created: u64,
        // }
        Ok(Self(VMValue::struct_(Struct::pack([
            VMValue::address(AccountAddress::ZERO),
            VMValue::vector_u8(digest.inner().iter().copied()),
            VMValue::u64(0),
            VMValue::u64(0),
            VMValue::u64(0),
        ]))))
    }

    pub fn one_time_witness() -> Result<Self, ExecutionError> {
        // public struct <ONE_TIME_WITNESS> has drop{
        //     _dummy: bool,
        // }
        Ok(Self(VMValue::struct_(Struct::pack([VMValue::bool(true)]))))
    }
}

//**************************************************************************************************
// Coin Functions
//**************************************************************************************************

impl Value {
    pub fn unpack_coin(self) -> Result<(ObjectID, u64), ExecutionError> {
        let [id, balance] = unpack(self.0)?;
        // unpack UID
        let [id] = unpack(id)?;
        // unpack ID
        let [id] = unpack(id)?;
        let id: AccountAddress = id.cast().map_err(iv("cast"))?;
        // unpack Balance
        let [balance] = unpack(balance)?;
        let balance: u64 = balance.cast().map_err(iv("cast"))?;
        Ok((ObjectID::from(id), balance))
    }

    pub fn coin_ref_value(self) -> Result<u64, ExecutionError> {
        let balance_value_ref = borrow_coin_ref_balance_value(self.0)?;
        let balance_value_ref: values::Reference = balance_value_ref.cast().map_err(iv("cast"))?;
        let balance_value = balance_value_ref.read_ref().map_err(iv("read ref"))?;
        balance_value.cast().map_err(iv("cast"))
    }

    /// The coin value MUST be checked before calling this function, if `amount` is greater than
    /// the value of the coin, it will return an invariant violation.
    pub fn coin_ref_subtract_balance(self, amount: u64) -> Result<(), ExecutionError> {
        coin_ref_modify_balance(self.0, |balance| {
            let Some(new_balance) = balance.checked_sub(amount) else {
                invariant_violation!("coin balance {balance} is less than {amount}")
            };
            Ok(new_balance)
        })
    }

    /// The coin max value MUST be checked before calling this function, if `amount` plus the current
    /// balance is greater than `u64::MAX`, it will return an invariant violation.
    pub fn coin_ref_add_balance(self, amount: u64) -> Result<(), ExecutionError> {
        coin_ref_modify_balance(self.0, |balance| {
            let Some(new_balance) = balance.checked_add(amount) else {
                invariant_violation!("coin balance {balance} + {amount} is greater than u64::MAX")
            };
            Ok(new_balance)
        })
    }
}

fn coin_ref_modify_balance(
    coin_ref: VMValue,
    modify: impl FnOnce(u64) -> Result<u64, ExecutionError>,
) -> Result<(), ExecutionError> {
    let balance_value_ref = borrow_coin_ref_balance_value(coin_ref)?;
    let reference: values::Reference = balance_value_ref.copy_value().cast().map_err(iv("cast"))?;
    let balance: u64 = reference
        .read_ref()
        .map_err(iv("read ref"))?
        .cast()
        .map_err(iv("cast"))?;
    let new_balance = modify(balance)?;
    let reference: values::Reference = balance_value_ref.cast().map_err(iv("cast"))?;
    reference
        .write_ref(VMValue::u64(new_balance))
        .map_err(iv("write ref"))
}

fn borrow_coin_ref_balance_value(coin_ref: VMValue) -> Result<VMValue, ExecutionError> {
    let coin_ref: values::StructRef = coin_ref.cast().map_err(iv("cast"))?;
    let balance = coin_ref.borrow_field(1).map_err(iv("borrow field"))?;
    let balance: values::StructRef = balance.cast().map_err(iv("cast"))?;
    balance.borrow_field(0).map_err(iv("borrow field"))
}

//**************************************************************************************************
// Upgrades
//**************************************************************************************************

impl Value {
    pub fn upgrade_cap(cap: UpgradeCap) -> Self {
        // public struct UpgradeCap has key, store {
        //     id: UID,
        //     package: ID,
        //     version: u64,
        //     policy: u8,
        // }
        let UpgradeCap {
            id,
            package,
            version,
            policy,
        } = cap;
        Self(VMValue::struct_(Struct::pack([
            Self::uid(id.id.bytes.into()).0,
            Self::id(package.bytes.into()).0,
            VMValue::u64(version),
            VMValue::u8(policy),
        ])))
    }

    pub fn upgrade_receipt(receipt: UpgradeReceipt) -> Self {
        // public struct UpgradeReceipt {
        //     cap: ID,
        //     package: ID,
        // }
        let UpgradeReceipt { cap, package } = receipt;
        Self(VMValue::struct_(Struct::pack([
            Self::id(cap.bytes.into()).0,
            Self::id(package.bytes.into()).0,
        ])))
    }

    pub fn into_upgrade_ticket(self) -> Result<UpgradeTicket, ExecutionError> {
        //  public struct UpgradeTicket {
        //     cap: ID,
        //     package: ID,
        //     policy: u8,
        //     digest: vector<u8>,
        // }
        // unpack UpgradeTicket
        let [cap, package, policy, digest] = unpack(self.0)?;
        // unpack cap ID
        let [cap] = unpack(cap)?;
        let cap: AccountAddress = cap.cast().map_err(iv("cast"))?;
        // unpack package ID
        let [package] = unpack(package)?;
        let package: AccountAddress = package.cast().map_err(iv("cast"))?;
        // unpack policy
        let policy: u8 = policy.cast().map_err(iv("cast"))?;
        // unpack digest
        let digest: Vec<u8> = digest.cast().map_err(iv("cast"))?;
        Ok(UpgradeTicket {
            cap: sui_types::id::ID::new(cap.into()),
            package: sui_types::id::ID::new(package.into()),
            policy,
            digest,
        })
    }
}

fn unpack<const N: usize>(value: VMValue) -> Result<[VMValue; N], ExecutionError> {
    let value: values::Struct = value.cast().map_err(iv("cast"))?;
    let unpacked = value.unpack().map_err(iv("unpack"))?.collect::<Vec<_>>();
    assert_invariant!(unpacked.len() == N, "Expected {N} fields, got {unpacked:?}");
    Ok(unpacked.try_into().unwrap())
}

const fn iv(case: &str) -> impl FnOnce(PartialVMError) -> ExecutionError + use<'_> {
    move |e| make_invariant_violation!("unexpected {case} failure {e:?}")
}
