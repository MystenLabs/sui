// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::base_types::{ObjectID, SuiAddress};

/// The shareable subset of `TransactionDenyConfig`, rules for transactions that
/// the node will refuse.
#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TransactionDenyRules {
    /// A list of object IDs that are not allowed to be accessed/used in transactions.
    /// Note that since this is checked during transaction signing, only root object ids
    /// are supported here (i.e. no child-objects).
    /// Similarly this does not apply to wrapped objects as they are not directly accessible.
    #[serde(default)]
    pub object_deny_list: BTreeSet<ObjectID>,

    /// A list of package object IDs that are not allowed to be called into in transactions,
    /// either directly or indirectly through transitive dependencies.
    /// Note that this does not apply to type arguments.
    /// Also since we only compare the deny list against the upgraded package ID of each dependency
    /// in the used package, when a package ID is denied, newer versions of that package are
    /// still allowed. If we want to deny the entire upgrade family of a package, we need to
    /// explicitly specify all the package IDs in the deny list.
    #[serde(default)]
    pub package_deny_list: BTreeSet<ObjectID>,

    /// A list of sui addresses that are not allowed to be used as the sender or sponsor.
    #[serde(default)]
    pub address_deny_list: BTreeSet<SuiAddress>,

    /// Whether publishing new packages is disabled.
    #[serde(default)]
    pub package_publish_disabled: bool,

    /// Whether upgrading existing packages is disabled.
    #[serde(default)]
    pub package_upgrade_disabled: bool,

    /// Whether usage of shared objects is disabled.
    #[serde(default)]
    pub shared_object_disabled: bool,

    /// Whether user transactions are disabled (i.e. only system transactions are allowed).
    /// This is essentially a kill switch for transactions processing to a degree.
    #[serde(default)]
    pub user_transaction_disabled: bool,

    /// Whether gasless transactions are disabled.
    #[serde(default)]
    pub gasless_disabled: bool,

    /// Whether receiving objects transferred to other objects is allowed.
    #[serde(default)]
    pub receiving_objects_disabled: bool,

    /// Whether zklogin transaction is disabled.
    #[serde(default)]
    pub zklogin_sig_disabled: bool,

    /// A list of disabled OAuth providers for zkLogin.
    #[serde(default)]
    pub zklogin_disabled_providers: BTreeSet<String>,
}

impl TransactionDenyRules {
    /// Maximum total entry count across all set fields permitted in a single
    /// peer-broadcast deny rules message.
    pub const MAX_SHARE_ENTRIES: usize = 5_000;

    /// Maximum length, in bytes, of any single zkLogin provider string accepted from
    /// a peer recommendation. Real provider names are short (e.g. "Google", "Apple").
    pub const MAX_ZKLOGIN_PROVIDER_LENGTH: usize = 256;

    /// Maximum BCS-serialized size of a shared rules payload.
    pub const MAX_SHARE_SERIALIZED_BYTES: u64 = 200 * 1024;

    /// Union-merge another rules set into this one. Set fields are extended; boolean
    /// fields are OR'd.
    pub fn merge(&mut self, other: &Self) {
        self.object_deny_list
            .extend(other.object_deny_list.iter().copied());
        self.package_deny_list
            .extend(other.package_deny_list.iter().copied());
        self.address_deny_list
            .extend(other.address_deny_list.iter().copied());
        self.package_publish_disabled |= other.package_publish_disabled;
        self.package_upgrade_disabled |= other.package_upgrade_disabled;
        self.shared_object_disabled |= other.shared_object_disabled;
        self.user_transaction_disabled |= other.user_transaction_disabled;
        self.gasless_disabled |= other.gasless_disabled;
        self.receiving_objects_disabled |= other.receiving_objects_disabled;
        self.zklogin_sig_disabled |= other.zklogin_sig_disabled;
        self.zklogin_disabled_providers
            .extend(other.zklogin_disabled_providers.iter().cloned());
    }

    /// Returns true if `self` contains every rule in `other`: each set field of
    /// `other` is a subset of `self`'s, and every boolean set in `other` is set in
    /// `self`. A peer's proposal "votes for" a pre-listed config only when it is a
    /// superset of that config's rules.
    pub fn is_superset_of(&self, other: &Self) -> bool {
        self.object_deny_list.is_superset(&other.object_deny_list)
            && self.package_deny_list.is_superset(&other.package_deny_list)
            && self.address_deny_list.is_superset(&other.address_deny_list)
            && self
                .zklogin_disabled_providers
                .is_superset(&other.zklogin_disabled_providers)
            && (self.package_publish_disabled || !other.package_publish_disabled)
            && (self.package_upgrade_disabled || !other.package_upgrade_disabled)
            && (self.shared_object_disabled || !other.shared_object_disabled)
            && (self.user_transaction_disabled || !other.user_transaction_disabled)
            && (self.gasless_disabled || !other.gasless_disabled)
            && (self.receiving_objects_disabled || !other.receiving_objects_disabled)
            && (self.zklogin_sig_disabled || !other.zklogin_sig_disabled)
    }

    /// Iterate the individual rule elements of this rules set: one item per deny-list
    /// entry and one per `true` boolean flag.
    pub fn elements(&self) -> impl Iterator<Item = DenyElement> + '_ {
        self.object_deny_list
            .iter()
            .map(|o| DenyElement::Object(*o))
            .chain(
                self.package_deny_list
                    .iter()
                    .map(|p| DenyElement::Package(*p)),
            )
            .chain(
                self.address_deny_list
                    .iter()
                    .map(|a| DenyElement::Address(*a)),
            )
            .chain(
                self.zklogin_disabled_providers
                    .iter()
                    .map(|p| DenyElement::ZkLoginProvider(p.clone())),
            )
            .chain(
                self.package_publish_disabled
                    .then_some(DenyElement::PackagePublishDisabled),
            )
            .chain(
                self.package_upgrade_disabled
                    .then_some(DenyElement::PackageUpgradeDisabled),
            )
            .chain(
                self.shared_object_disabled
                    .then_some(DenyElement::SharedObjectDisabled),
            )
            .chain(
                self.user_transaction_disabled
                    .then_some(DenyElement::UserTransactionDisabled),
            )
            .chain(
                self.gasless_disabled
                    .then_some(DenyElement::GaslessDisabled),
            )
            .chain(
                self.receiving_objects_disabled
                    .then_some(DenyElement::ReceivingObjectsDisabled),
            )
            .chain(
                self.zklogin_sig_disabled
                    .then_some(DenyElement::ZkLoginSigDisabled),
            )
    }

    /// Apply a single rule element to this rules set.
    pub fn apply_element(&mut self, element: &DenyElement) {
        match element {
            DenyElement::Object(o) => {
                self.object_deny_list.insert(*o);
            }
            DenyElement::Package(p) => {
                self.package_deny_list.insert(*p);
            }
            DenyElement::Address(a) => {
                self.address_deny_list.insert(*a);
            }
            DenyElement::ZkLoginProvider(p) => {
                self.zklogin_disabled_providers.insert(p.clone());
            }
            DenyElement::PackagePublishDisabled => self.package_publish_disabled = true,
            DenyElement::PackageUpgradeDisabled => self.package_upgrade_disabled = true,
            DenyElement::SharedObjectDisabled => self.shared_object_disabled = true,
            DenyElement::UserTransactionDisabled => self.user_transaction_disabled = true,
            DenyElement::GaslessDisabled => self.gasless_disabled = true,
            DenyElement::ReceivingObjectsDisabled => self.receiving_objects_disabled = true,
            DenyElement::ZkLoginSigDisabled => self.zklogin_sig_disabled = true,
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn entry_count(&self) -> usize {
        self.object_deny_list.len()
            + self.package_deny_list.len()
            + self.address_deny_list.len()
            + self.zklogin_disabled_providers.len()
    }

    /// Reject rules that are too large to share via consensus. Used by both the
    /// admin endpoint (before submission) and the consensus validator (on receive)
    /// so the limits can't drift between the two checks.
    pub fn check_share_limits(&self) -> Result<(), String> {
        if self.entry_count() > Self::MAX_SHARE_ENTRIES {
            return Err(format!(
                "rules entry count {} exceeds limit ({})",
                self.entry_count(),
                Self::MAX_SHARE_ENTRIES,
            ));
        }
        for provider in &self.zklogin_disabled_providers {
            if provider.len() > Self::MAX_ZKLOGIN_PROVIDER_LENGTH {
                return Err(format!(
                    "zklogin provider name too long: {} bytes (max {})",
                    provider.len(),
                    Self::MAX_ZKLOGIN_PROVIDER_LENGTH,
                ));
            }
        }
        let serialized_size = bcs::serialized_size(self)
            .map_err(|e| format!("failed to compute rules serialized size: {e}"))?
            as u64;
        if serialized_size > Self::MAX_SHARE_SERIALIZED_BYTES {
            return Err(format!(
                "rules serialized size {serialized_size} bytes exceeds limit ({})",
                Self::MAX_SHARE_SERIALIZED_BYTES,
            ));
        }
        Ok(())
    }
}

/// A single indivisible rule within a `TransactionDenyRules`: one deny-list entry or
/// one boolean kill switch. The "default" deny-config bucket votes on these
/// individually.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DenyElement {
    Object(ObjectID),
    Package(ObjectID),
    Address(SuiAddress),
    ZkLoginProvider(String),
    PackagePublishDisabled,
    PackageUpgradeDisabled,
    SharedObjectDisabled,
    UserTransactionDisabled,
    GaslessDisabled,
    ReceivingObjectsDisabled,
    ZkLoginSigDisabled,
}

impl DenyElement {
    pub fn kind(&self) -> DenyElementKind {
        match self {
            DenyElement::Object(_) => DenyElementKind::Object,
            DenyElement::Package(_) => DenyElementKind::Package,
            DenyElement::Address(_) => DenyElementKind::Address,
            DenyElement::ZkLoginProvider(_) => DenyElementKind::ZkLoginProvider,
            DenyElement::PackagePublishDisabled => DenyElementKind::PackagePublishDisabled,
            DenyElement::PackageUpgradeDisabled => DenyElementKind::PackageUpgradeDisabled,
            DenyElement::SharedObjectDisabled => DenyElementKind::SharedObjectDisabled,
            DenyElement::UserTransactionDisabled => DenyElementKind::UserTransactionDisabled,
            DenyElement::GaslessDisabled => DenyElementKind::GaslessDisabled,
            DenyElement::ReceivingObjectsDisabled => DenyElementKind::ReceivingObjectsDisabled,
            DenyElement::ZkLoginSigDisabled => DenyElementKind::ZkLoginSigDisabled,
        }
    }
}

/// A `DenyElement` stripped of its payload — the category that the per-kind default
/// buckets in `PeerDenySyncConfig` partition over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DenyElementKind {
    Object,
    Package,
    Address,
    ZkLoginProvider,
    PackagePublishDisabled,
    PackageUpgradeDisabled,
    SharedObjectDisabled,
    UserTransactionDisabled,
    GaslessDisabled,
    ReceivingObjectsDisabled,
    ZkLoginSigDisabled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base_types::dbg_addr;

    #[test]
    fn merge_unions_sets_and_ors_bools() {
        let mut a = TransactionDenyRules::default();
        a.object_deny_list.insert(ObjectID::from_single_byte(1));
        a.address_deny_list.insert(dbg_addr(2));
        a.package_publish_disabled = true;

        let mut b = TransactionDenyRules::default();
        b.object_deny_list.insert(ObjectID::from_single_byte(3));
        b.address_deny_list.insert(dbg_addr(2));
        b.user_transaction_disabled = true;
        b.zklogin_disabled_providers.insert("Google".to_string());

        a.merge(&b);

        assert_eq!(a.object_deny_list.len(), 2);
        assert!(a.object_deny_list.contains(&ObjectID::from_single_byte(1)));
        assert!(a.object_deny_list.contains(&ObjectID::from_single_byte(3)));
        assert_eq!(a.address_deny_list.len(), 1);
        assert!(a.package_publish_disabled);
        assert!(a.user_transaction_disabled);
        assert!(a.zklogin_disabled_providers.contains("Google"));
    }

    #[test]
    fn entry_count_sums_set_lengths() {
        let mut r = TransactionDenyRules::default();
        r.object_deny_list.insert(ObjectID::from_single_byte(1));
        r.object_deny_list.insert(ObjectID::from_single_byte(2));
        r.package_deny_list.insert(ObjectID::from_single_byte(3));
        r.address_deny_list.insert(dbg_addr(4));
        r.zklogin_disabled_providers.insert("a".to_string());
        assert_eq!(r.entry_count(), 5);
    }

    #[test]
    fn is_superset_of_sets_and_bools() {
        let mut larger = TransactionDenyRules::default();
        larger
            .object_deny_list
            .insert(ObjectID::from_single_byte(1));
        larger
            .object_deny_list
            .insert(ObjectID::from_single_byte(2));
        larger.package_publish_disabled = true;

        let mut smaller = TransactionDenyRules::default();
        smaller
            .object_deny_list
            .insert(ObjectID::from_single_byte(1));
        smaller.package_publish_disabled = true;

        assert!(larger.is_superset_of(&smaller));
        assert!(larger.is_superset_of(&TransactionDenyRules::default()));
        assert!(!smaller.is_superset_of(&larger));

        // A boolean set in `other` but not `self` breaks the superset relation.
        let mut other_bool = TransactionDenyRules::default();
        other_bool
            .object_deny_list
            .insert(ObjectID::from_single_byte(1));
        other_bool.user_transaction_disabled = true;
        assert!(!larger.is_superset_of(&other_bool));
    }

    #[test]
    fn deny_element_kind_matches_variant() {
        let mut rules = TransactionDenyRules::default();
        rules.object_deny_list.insert(ObjectID::from_single_byte(1));
        rules
            .package_deny_list
            .insert(ObjectID::from_single_byte(2));
        rules.address_deny_list.insert(dbg_addr(3));
        rules
            .zklogin_disabled_providers
            .insert("Google".to_string());
        rules.package_publish_disabled = true;
        rules.package_upgrade_disabled = true;
        rules.shared_object_disabled = true;
        rules.user_transaction_disabled = true;
        rules.gasless_disabled = true;
        rules.receiving_objects_disabled = true;
        rules.zklogin_sig_disabled = true;

        let expected_kinds = [
            DenyElementKind::Object,
            DenyElementKind::Package,
            DenyElementKind::Address,
            DenyElementKind::ZkLoginProvider,
            DenyElementKind::PackagePublishDisabled,
            DenyElementKind::PackageUpgradeDisabled,
            DenyElementKind::SharedObjectDisabled,
            DenyElementKind::UserTransactionDisabled,
            DenyElementKind::GaslessDisabled,
            DenyElementKind::ReceivingObjectsDisabled,
            DenyElementKind::ZkLoginSigDisabled,
        ];

        let actual_kinds: BTreeSet<DenyElementKind> = rules.elements().map(|e| e.kind()).collect();
        let expected: BTreeSet<DenyElementKind> = expected_kinds.iter().copied().collect();
        assert_eq!(actual_kinds, expected);
    }

    /// A payload within the entry and per-string caps can still exceed the wire
    /// budget: ~1,100 max-length provider strings BCS-encode past the serialized-size
    /// limit while sitting at a fifth of the entry cap.
    #[test]
    fn check_share_limits_bounds_serialized_size() {
        let rules = TransactionDenyRules {
            zklogin_disabled_providers: (0..1_100)
                .map(|i| {
                    format!(
                        "{i:0>width$}",
                        width = TransactionDenyRules::MAX_ZKLOGIN_PROVIDER_LENGTH
                    )
                })
                .collect(),
            ..Default::default()
        };
        assert!(rules.entry_count() < TransactionDenyRules::MAX_SHARE_ENTRIES);
        let err = rules.check_share_limits().unwrap_err();
        assert!(err.contains("serialized size"), "{err}");

        // A maximal all-ID config stays within the byte budget: the entry cap is the
        // binding limit for fixed-size entries.
        let max_ids = TransactionDenyRules {
            object_deny_list: (0..TransactionDenyRules::MAX_SHARE_ENTRIES)
                .map(|i| {
                    let mut bytes = [0u8; ObjectID::LENGTH];
                    bytes[..8].copy_from_slice(&(i as u64).to_be_bytes());
                    ObjectID::from_bytes(bytes).unwrap()
                })
                .collect(),
            ..Default::default()
        };
        assert_eq!(
            max_ids.entry_count(),
            TransactionDenyRules::MAX_SHARE_ENTRIES
        );
        max_ids.check_share_limits().unwrap();
    }

    #[test]
    fn elements_round_trips_through_apply_element() {
        let mut original = TransactionDenyRules::default();
        original
            .object_deny_list
            .insert(ObjectID::from_single_byte(1));
        original
            .package_deny_list
            .insert(ObjectID::from_single_byte(2));
        original.address_deny_list.insert(dbg_addr(3));
        original
            .zklogin_disabled_providers
            .insert("Google".to_string());
        original.package_publish_disabled = true;
        original.user_transaction_disabled = true;
        original.zklogin_sig_disabled = true;

        let elements: Vec<DenyElement> = original.elements().collect();
        assert_eq!(elements.len(), 7);

        let mut rebuilt = TransactionDenyRules::default();
        for element in &elements {
            rebuilt.apply_element(element);
        }
        assert_eq!(rebuilt, original);
    }
}
