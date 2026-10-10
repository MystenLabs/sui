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
    crypto::DefaultHash,
    dynamic_field::{Field, derive_dynamic_field_id},
    object::Object,
};
use fastcrypto::hash::HashFunction;
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

pub const ACCOUNT_POLICY_MODULE_NAME: &IdentStr = ident_str!("account_policy");
pub const POLICY_KEY_STRUCT_NAME: &IdentStr = ident_str!("PolicyKey");
pub const SPENT_STRUCT_NAME: &IdentStr = ident_str!("Spent");
pub const CUSTODY_STRUCT_NAME: &IdentStr = ident_str!("Custody");

fn account_policy_struct(name: &IdentStr, type_params: Vec<TypeTag>) -> TypeTag {
    TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ACCOUNT_POLICY_MODULE_NAME.to_owned(),
        name: name.to_owned(),
        type_params,
    }))
}

/// Accumulator type of a policy's per-epoch spend counter for `inner`: a coin type for coin
/// outflow, or `Custody` for objects taken by a package.
pub fn spent_type_tag(inner: TypeTag) -> TypeTag {
    account_policy_struct(SPENT_STRUCT_NAME, vec![inner])
}

pub fn custody_type_tag() -> TypeTag {
    account_policy_struct(CUSTODY_STRUCT_NAME, vec![])
}

/// Whether `type_` is a policy spend counter, which the accumulator machinery settles like a
/// balance.
pub fn is_spent_type(type_: &TypeTag) -> bool {
    matches!(type_, TypeTag::Struct(tag)
        if tag.address == SUI_FRAMEWORK_ADDRESS
            && tag.module.as_ident_str() == ACCOUNT_POLICY_MODULE_NAME
            && tag.name.as_ident_str() == SPENT_STRUCT_NAME)
}

/// Accumulator address of a policy's spend counters: one per owner and epoch, and per package
/// for custody counts. A fresh address each epoch is what resets the budget.
pub fn counter_address(owner: SuiAddress, epoch: u64, package: Option<ObjectID>) -> SuiAddress {
    let mut hasher = DefaultHash::default();
    hasher.update(b"sui::account_policy::counter");
    bcs::serialize_into(&mut hasher, &(owner, epoch, package))
        .expect("counter key serialization cannot fail");
    SuiAddress::from_bytes(hasher.finalize().digest).expect("digest is address sized")
}

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
    /// Maximum number of the owner's objects the package may take per epoch, if bounded.
    pub custody_limit: Option<u64>,
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
    /// Per-epoch net outflow limit by coin type string, gas included for SUI. Types without an
    /// entry may not flow out at all.
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
                .package_permission(original_package_id)
                .is_some_and(|permission| permission.allows_custody_of(object_type))
    }

    /// The per-epoch cap on objects `original_package_id` may take, if the policy sets one.
    pub fn custody_limit(&self, original_package_id: ObjectID) -> Option<u64> {
        self.package_permission(original_package_id)
            .and_then(|permission| permission.custody_limit)
    }

    fn package_permission(&self, original_package_id: ObjectID) -> Option<&PackagePermission> {
        self.packages
            .contents
            .iter()
            .find(|entry| entry.key == original_package_id)
            .map(|entry| &entry.value)
    }
}

/// Policy type strings are user supplied, so they are compared as parsed types; an unparseable
/// string matches nothing.
fn type_string_matches(type_string: &str, type_tag: &TypeTag) -> bool {
    TypeTag::from_str(type_string).is_ok_and(|parsed| parsed == *type_tag)
}
