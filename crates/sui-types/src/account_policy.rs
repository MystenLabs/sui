// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Account policies: an opt-in, per-address rule set that bounds what a transaction signed by the
//! account key alone may do. Policies live as dynamic fields of the `AccountPolicyRegistry`
//! system object, which every transaction implicitly reads at the version consensus assigned, so
//! execution can look up the sender's policy deterministically and reject the transaction if the
//! policy is active and any rule is violated. A transaction co-signed by the policy's guardian is
//! exempt.

use std::str::FromStr;

use crate::{
    SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_PACKAGE_ID,
    base_types::{ObjectID, SuiAddress},
    collection_types::{VecMap, VecSet},
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

/// Mirrors `sui::account_policy::PackagePermission`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagePermission {
    /// The package may delete, wrap, or give away the owner's objects.
    pub custody: bool,
    /// Object types custody is limited to, as type strings; empty means any type.
    pub custody_types: Vec<String>,
}

impl PackagePermission {
    fn allows_custody_of(&self, object_type: &TypeTag) -> bool {
        self.custody
            && (self.custody_types.is_empty()
                || self
                    .custody_types
                    .iter()
                    .any(|allowed| type_string_matches(allowed, object_type)))
    }
}

/// Mirrors `sui::account_policy::AccountPolicy`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPolicy {
    pub owner: SuiAddress,
    pub guardian: SuiAddress,
    pub gas_budget_cap: u64,
    /// First epoch in which the policy is enforced. `u64::MAX` means disabled.
    pub activation_epoch: u64,
    /// Per-transaction net outflow limit by coin type string. Types without an entry may not
    /// flow out at all.
    pub coin_limits: VecMap<String, u64>,
    /// Addresses (or object IDs) that coins and objects may be sent to without limit.
    pub recipients: VecSet<SuiAddress>,
    /// Packages that may be called, by original package ID.
    pub packages: VecMap<ObjectID, PackagePermission>,
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

    /// A transaction co-signed by the guardian is exempt from the policy.
    pub fn is_guardian_approved(&self, co_signers: &[SuiAddress]) -> bool {
        co_signers.contains(&self.guardian)
    }

    pub fn coin_limit(&self, coin_type: &TypeTag) -> u64 {
        self.coin_limits
            .contents
            .iter()
            .find(|entry| type_string_matches(&entry.key, coin_type))
            .map_or(0, |entry| entry.value)
    }

    pub fn is_recipient(&self, address: SuiAddress) -> bool {
        self.recipients.contents.contains(&address)
    }

    /// Whether a package may be called. The system package is always callable: it only moves
    /// value between the sender's coins and the sender's stake.
    pub fn allows_package(&self, original_package_id: ObjectID) -> bool {
        original_package_id == SUI_SYSTEM_PACKAGE_ID
            || self
                .packages
                .contents
                .iter()
                .any(|entry| entry.key == original_package_id)
    }

    /// Whether a package may take an object of `object_type` out of the owner's possession.
    pub fn allows_custody(&self, original_package_id: ObjectID, object_type: &TypeTag) -> bool {
        original_package_id == SUI_SYSTEM_PACKAGE_ID
            || self
                .packages
                .contents
                .iter()
                .find(|entry| entry.key == original_package_id)
                .is_some_and(|entry| entry.value.allows_custody_of(object_type))
    }
}

/// Policy type strings are user supplied, so they are compared as parsed types; an unparseable
/// string matches nothing.
fn type_string_matches(type_string: &str, type_tag: &TypeTag) -> bool {
    TypeTag::from_str(type_string).is_ok_and(|parsed| parsed == *type_tag)
}
