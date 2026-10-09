// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Account policies: an opt-in, per-address rule set that bounds what a transaction signed by the
//! account key alone may do. Policies live as dynamic fields of the `AccountPolicyRegistry`
//! system object, which every transaction implicitly reads at the version consensus assigned, so
//! execution can look up the sender's policy deterministically and reject the transaction if the
//! policy is active and any rule is violated.

use crate::{
    SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_PACKAGE_ID,
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
    dynamic_field::{Field, derive_dynamic_field_id},
    object::Object,
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

pub const ACCOUNT_POLICY_MODULE_NAME: &IdentStr = ident_str!("account_policy");
pub const POLICY_KEY_STRUCT_NAME: &IdentStr = ident_str!("PolicyKey");

/// Packages a transaction bound by an active policy may call. The system package only moves
/// value between the sender's coins and the sender's stake, so it can never take anything out of
/// the account.
pub const ACCOUNT_POLICY_ALLOWED_PACKAGES: &[ObjectID] = &[SUI_SYSTEM_PACKAGE_ID];

/// Mirrors `sui::account_policy::PolicyKey`, the dynamic field key of an owner's policy.
#[derive(Serialize, Deserialize)]
pub struct PolicyKey(pub SuiAddress);

/// The ID of the dynamic field holding `owner`'s policy, whether or not it exists.
pub fn account_policy_field_id(owner: SuiAddress) -> ObjectID {
    let key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ACCOUNT_POLICY_MODULE_NAME.to_owned(),
        name: POLICY_KEY_STRUCT_NAME.to_owned(),
        type_params: vec![],
    }));
    derive_dynamic_field_id(
        SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID,
        &key_type,
        &bcs::to_bytes(&PolicyKey(owner)).expect("address serialization cannot fail"),
    )
    .expect("key type serialization cannot fail")
}

/// Mirrors `sui::account_policy::AccountPolicy`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPolicy {
    pub owner: SuiAddress,
    pub guardian: SuiAddress,
    /// Maximum net SUI (in MIST) that may leave the owner's coins and stake in one transaction.
    pub sui_limit_per_tx: u64,
    pub gas_budget_cap: u64,
    /// First epoch in which the policy is enforced. `u64::MAX` means disabled.
    pub activation_epoch: u64,
    /// Transaction digests the guardian has exempted from the policy.
    pub approved_digests: Vec<Vec<u8>>,
}

impl AccountPolicy {
    /// Parses the policy out of its dynamic field object.
    pub fn from_field_object(object: &Object) -> Option<Self> {
        let field: Field<PolicyKey, AccountPolicy> =
            bcs::from_bytes(object.data.try_as_move()?.contents()).ok()?;
        Some(field.value)
    }

    pub fn is_active(&self, epoch: u64) -> bool {
        self.activation_epoch <= epoch
    }

    pub fn is_approved(&self, digest: &TransactionDigest) -> bool {
        self.approved_digests
            .iter()
            .any(|approved| approved.as_slice() == digest.inner().as_slice())
    }
}
