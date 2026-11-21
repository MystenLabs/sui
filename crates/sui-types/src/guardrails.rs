// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Guardrails bound how much of each balance an account's key alone can move per epoch. See
//! `crates/sui-core/src/guardrails/DESIGN.md`.
//!
//! The rules of an account are its `Policy`, a shared object whose ID is derived from the owner
//! address. Every transaction sent by an account with an active policy takes that policy as a
//! mutable shared input the transaction did not declare. Consensus assigns its version, so the
//! account's transactions execute in sequence and each records its spend in the object.

use std::collections::BTreeSet;

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

use crate::{
    SUI_FRAMEWORK_ADDRESS, SUI_GUARDRAILS_REGISTRY_OBJECT_ID,
    base_types::{ObjectID, SuiAddress},
    collection_types::{Entry, VecMap},
    committee::EpochId,
    derived_object::derive_object_id,
    id::UID,
    object::Object,
    transaction::{
        SharedInputObject, SharedObjectMutability, TransactionData, TransactionDataAPI,
        TransactionKind, WithdrawFrom,
    },
};
use sui_protocol_config::ProtocolConfig;

pub const GUARDRAILS_MODULE_NAME: &IdentStr = ident_str!("guardrails");
pub const POLICY_STRUCT_NAME: &IdentStr = ident_str!("Policy");
pub const POLICY_KEY_STRUCT_NAME: &IdentStr = ident_str!("PolicyKey");

fn guardrails_struct(name: &IdentStr) -> StructTag {
    StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: GUARDRAILS_MODULE_NAME.to_owned(),
        name: name.to_owned(),
        type_params: vec![],
    }
}

/// Mirrors `sui::guardrails::PolicyKey`, the derived-object key of an owner's policy.
#[derive(Serialize, Deserialize)]
struct PolicyKey(SuiAddress);

/// The ID of `owner`'s policy object, whether or not it has been created.
pub fn policy_object_id(owner: SuiAddress) -> ObjectID {
    derive_object_id(
        SUI_GUARDRAILS_REGISTRY_OBJECT_ID,
        &TypeTag::Struct(Box::new(guardrails_struct(POLICY_KEY_STRUCT_NAME))),
        &bcs::to_bytes(&PolicyKey(owner)).expect("address serialization cannot fail"),
    )
    .expect("key type serialization cannot fail")
}

/// Mirrors `sui::guardrails::Policy`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub id: UID,
    pub owner: SuiAddress,
    pub guardian: SuiAddress,
    /// First epoch in which the policy is enforced.
    pub active_from: u64,
    /// First epoch in which the policy is no longer enforced.
    pub active_until: u64,
    /// Most of each coin type that may leave the account per epoch, keyed as
    /// `std::type_name::with_defining_ids` prints the type.
    pub limits: VecMap<String, u64>,
    pub recovery_delay_epochs: u64,
    /// The epoch `spent` belongs to. Spend from an earlier epoch is stale.
    pub spent_epoch: u64,
    /// What left the account in `spent_epoch`, keyed as `limits` is.
    pub spent: VecMap<String, u64>,
}

/// How `std::type_name` prints `coin_type`, which is how `Policy` keys its limits and spend.
fn type_key(coin_type: &TypeTag) -> String {
    coin_type.to_canonical_string(/* with_prefix */ false)
}

impl Policy {
    pub fn is_policy(object: &Object) -> bool {
        object
            .struct_tag()
            .is_some_and(|tag| tag == guardrails_struct(POLICY_STRUCT_NAME))
    }

    pub fn from_object(object: &Object) -> Option<Self> {
        if !Self::is_policy(object) {
            return None;
        }
        bcs::from_bytes(object.data.try_as_move()?.contents()).ok()
    }

    /// Serializes the policy back into `object`, which must be its own object.
    pub fn write_to(&self, object: &mut Object) {
        let contents = bcs::to_bytes(self).expect("policy serialization cannot fail");
        object
            .data
            .try_as_move_mut()
            .expect("a policy is a Move object")
            .set_contents_unsafe(contents);
    }

    pub fn is_active(&self, epoch: u64) -> bool {
        self.active_from <= epoch && epoch < self.active_until
    }

    /// A transaction the guardian sponsors is exempt from the policy.
    pub fn exempts(&self, gas_owner: SuiAddress) -> bool {
        gas_owner == self.guardian
    }

    /// The per-epoch limit of `coin_type`. A type without an entry cannot leave at all.
    pub fn limit(&self, coin_type: &TypeTag) -> u64 {
        self.limits.get(&type_key(coin_type)).copied().unwrap_or(0)
    }

    /// What left the account in `epoch` so far.
    pub fn spent(&self, epoch: u64, coin_type: &TypeTag) -> u64 {
        if self.spent_epoch != epoch {
            return 0;
        }
        self.spent.get(&type_key(coin_type)).copied().unwrap_or(0)
    }

    /// Adds `amount` to what left the account in `epoch`, starting over if the epoch moved on.
    /// Does not check the limit.
    pub fn record(&mut self, epoch: u64, coin_type: &TypeTag, amount: u64) {
        if self.spent_epoch != epoch {
            self.spent_epoch = epoch;
            self.spent.contents.clear();
        }
        let key = type_key(coin_type);
        match self.spent.contents.iter_mut().find(|e| e.key == key) {
            Some(entry) => entry.value = entry.value.saturating_add(amount),
            None => self.spent.contents.push(Entry { key, value: amount }),
        }
    }
}

/// The policy consensus adds to `tx_data` as a mutable shared input: the sender's, when the
/// transaction does not declare it and the policy is active in `epoch`. `latest` reads the
/// latest version of an object.
///
/// Every validator reaches the same answer from its own latest state. Whether a policy is
/// active in an epoch never changes within that epoch, since enabling and disabling take effect
/// at later epochs, and the initial shared version of a policy never changes.
pub fn implicit_policy_input(
    protocol_config: &ProtocolConfig,
    epoch: EpochId,
    tx_data: &TransactionData,
    latest: impl FnOnce(&ObjectID) -> Option<Object>,
) -> Option<SharedInputObject> {
    if !protocol_config.enable_guardrails() {
        return None;
    }
    let id = sender_policy_id(tx_data.kind(), tx_data.sender())?;
    if tx_data
        .shared_input_objects()
        .iter()
        .any(|input| input.id == id)
    {
        return None;
    }
    let object = latest(&id)?;
    if !Policy::from_object(&object)?.is_active(epoch) {
        return None;
    }
    Some(SharedInputObject {
        id,
        initial_shared_version: object.owner().start_version()?,
        mutability: SharedObjectMutability::Mutable,
    })
}

/// The policy that binds a transaction: its sender's. `None` for anything but a user
/// transaction. The policy may not exist.
pub fn sender_policy_id(kind: &TransactionKind, sender: SuiAddress) -> Option<ObjectID> {
    matches!(kind, TransactionKind::ProgrammableTransaction(_)).then(|| policy_object_id(sender))
}

/// The accounts other than `sender` whose address balance the transaction withdraws from under
/// an allowance.
pub fn allowance_funders(kind: &TransactionKind, sender: SuiAddress) -> BTreeSet<SuiAddress> {
    kind.get_funds_withdrawals()
        .filter_map(|withdrawal| match &withdrawal.withdraw_from {
            WithdrawFrom::SenderAllowance { funder, .. } if *funder != sender => Some(*funder),
            _ => None,
        })
        .collect()
}
