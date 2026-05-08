// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sui_types::base_types::{AuthorityName, ObjectID, SuiAddress};
pub use sui_types::transaction_deny_rules::TransactionDenyRules;

use crate::dynamic_transaction_signing_checks::{
    DynamicCheckRunnerContext, DynamicCheckRunnerError,
};

/// Configuration for sharing recommended `TransactionDenyConfig` settings with peers via
/// consensus. Operators define an allowlist of trusted peer authorities; updates from
/// allowlisted peers are union/OR-merged with the local config to form the effective
/// config used at signing time.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PeerDenySyncConfig {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub peer_allowlist: BTreeSet<AuthorityName>,

    #[serde(default)]
    pub broadcast_on_startup: bool,

    #[serde(default)]
    pub broadcast_on_epoch_change: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TransactionDenyConfig {
    /// All shareable settings live here. Flattened so the YAML schema is unchanged.
    #[serde(flatten)]
    rules: TransactionDenyRules,

    /// Dynamic transaction checks to run on transactions.
    /// Program is loaded at deserialization time to ensure that any syntactic issues are caught
    /// immediately.
    /// Local-only: never propagated through the consensus-shared recommendation flow.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::dynamic_transaction_signing_checks::serialize_dynamic_transaction_checks",
        deserialize_with = "crate::dynamic_transaction_signing_checks::deserialize_dynamic_transaction_checks"
    )]
    dynamic_transaction_checks: Option<DynamicCheckRunnerContext>,
}

impl TransactionDenyConfig {
    pub fn rules(&self) -> &TransactionDenyRules {
        &self.rules
    }

    pub fn get_object_deny_set(&self) -> &BTreeSet<ObjectID> {
        &self.rules.object_deny_list
    }

    pub fn get_package_deny_set(&self) -> &BTreeSet<ObjectID> {
        &self.rules.package_deny_list
    }

    pub fn get_address_deny_set(&self) -> &BTreeSet<SuiAddress> {
        &self.rules.address_deny_list
    }

    pub fn package_publish_disabled(&self) -> bool {
        self.rules.package_publish_disabled
    }

    pub fn package_upgrade_disabled(&self) -> bool {
        self.rules.package_upgrade_disabled
    }

    pub fn shared_object_disabled(&self) -> bool {
        self.rules.shared_object_disabled
    }

    pub fn user_transaction_disabled(&self) -> bool {
        self.rules.user_transaction_disabled
    }

    pub fn gasless_disabled(&self) -> bool {
        self.rules.gasless_disabled
    }

    pub fn receiving_objects_disabled(&self) -> bool {
        self.rules.receiving_objects_disabled
    }

    pub fn zklogin_sig_disabled(&self) -> bool {
        self.rules.zklogin_sig_disabled
    }

    pub fn zklogin_disabled_providers(&self) -> &BTreeSet<String> {
        &self.rules.zklogin_disabled_providers
    }

    pub fn dynamic_transaction_checks(&self) -> &Option<DynamicCheckRunnerContext> {
        &self.dynamic_transaction_checks
    }

    pub fn has_dynamic_transaction_checks(&self) -> bool {
        self.dynamic_transaction_checks.is_some()
    }

    /// Build the effective merged config from a local config and an iterator of peer
    /// rules. The local rules form the base; each peer rules entry is OR/union-merged on
    /// top. `dynamic_transaction_checks` is taken verbatim from `local` (local-only).
    pub fn from_local_and_peers<'a>(
        local: &Self,
        peer_rules: impl Iterator<Item = &'a TransactionDenyRules>,
    ) -> Self {
        let mut rules = local.rules.clone();
        for peer in peer_rules {
            rules.merge(peer);
        }
        Self {
            rules,
            dynamic_transaction_checks: local.dynamic_transaction_checks.clone(),
        }
    }
}

#[derive(Default)]
pub struct TransactionDenyConfigBuilder {
    config: TransactionDenyConfig,
}

impl TransactionDenyConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(self) -> TransactionDenyConfig {
        self.config
    }

    pub fn disable_user_transaction(mut self) -> Self {
        self.config.rules.user_transaction_disabled = true;
        self
    }

    pub fn disable_gasless(mut self) -> Self {
        self.config.rules.gasless_disabled = true;
        self
    }

    pub fn disable_shared_object_transaction(mut self) -> Self {
        self.config.rules.shared_object_disabled = true;
        self
    }

    pub fn disable_package_publish(mut self) -> Self {
        self.config.rules.package_publish_disabled = true;
        self
    }

    pub fn disable_package_upgrade(mut self) -> Self {
        self.config.rules.package_upgrade_disabled = true;
        self
    }

    pub fn disable_receiving_objects(mut self) -> Self {
        self.config.rules.receiving_objects_disabled = true;
        self
    }

    pub fn add_denied_object(mut self, id: ObjectID) -> Self {
        self.config.rules.object_deny_list.insert(id);
        self
    }

    pub fn add_denied_address(mut self, address: SuiAddress) -> Self {
        self.config.rules.address_deny_list.insert(address);
        self
    }

    pub fn add_denied_package(mut self, id: ObjectID) -> Self {
        self.config.rules.package_deny_list.insert(id);
        self
    }

    pub fn disable_zklogin_sig(mut self) -> Self {
        self.config.rules.zklogin_sig_disabled = true;
        self
    }

    pub fn add_zklogin_disabled_provider(mut self, provider: String) -> Self {
        self.config
            .rules
            .zklogin_disabled_providers
            .insert(provider);
        self
    }

    pub fn add_dynamic_transaction_checks(
        mut self,
        checks: String,
    ) -> Result<Self, DynamicCheckRunnerError> {
        self.config.dynamic_transaction_checks = Some(DynamicCheckRunnerContext::new(checks)?);
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(byte: u8) -> ObjectID {
        ObjectID::new([byte; 32])
    }

    #[test]
    fn from_local_and_peers_preserves_dynamic_checks_from_local_only() {
        let local = TransactionDenyConfigBuilder::new()
            .add_denied_object(obj(1))
            .build();
        let mut peer = TransactionDenyRules::default();
        peer.object_deny_list.insert(obj(2));
        peer.user_transaction_disabled = true;

        let merged = TransactionDenyConfig::from_local_and_peers(&local, std::iter::once(&peer));

        assert!(merged.get_object_deny_set().contains(&obj(1)));
        assert!(merged.get_object_deny_set().contains(&obj(2)));
        assert!(merged.user_transaction_disabled());
        assert!(!merged.has_dynamic_transaction_checks());
    }

    #[test]
    fn yaml_round_trip_preserves_existing_schema() {
        // Older operator configs use kebab-case fields at the top level of
        // transaction-deny-config; the flatten attribute must keep that schema.
        let yaml = r#"
            package-publish-disabled: true
            user-transaction-disabled: false
            object-deny-list:
              - "0x0101010101010101010101010101010101010101010101010101010101010101"
        "#;
        let cfg: TransactionDenyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.package_publish_disabled());
        assert!(!cfg.user_transaction_disabled());
        assert_eq!(cfg.get_object_deny_set().len(), 1);
    }

    /// Forward round-trip with every collection field populated. Catches schema drift
    /// between `#[serde(flatten)] rules` and the custom (de)serializer on
    /// `dynamic_transaction_checks` — a refactor that touches either is most likely
    /// to trip here.
    #[test]
    fn yaml_full_round_trip_preserves_all_fields() {
        let cfg = TransactionDenyConfigBuilder::new()
            .add_denied_object(obj(1))
            .add_denied_object(obj(2))
            .add_denied_package(obj(3))
            .add_denied_address(SuiAddress::from_bytes([4u8; 32]).unwrap())
            .disable_user_transaction()
            .disable_gasless()
            .disable_shared_object_transaction()
            .disable_package_publish()
            .disable_package_upgrade()
            .disable_receiving_objects()
            .disable_zklogin_sig()
            .add_zklogin_disabled_provider("Google".to_string())
            .add_zklogin_disabled_provider("Apple".to_string())
            .build();

        let yaml = serde_yaml::to_string(&cfg).expect("serialize");
        let parsed: TransactionDenyConfig = serde_yaml::from_str(&yaml).expect("deserialize");

        // Every field round-trips. Compare via the public accessors since
        // TransactionDenyConfig doesn't derive PartialEq (Starlark context).
        assert_eq!(cfg.rules(), parsed.rules());
        assert_eq!(
            cfg.has_dynamic_transaction_checks(),
            parsed.has_dynamic_transaction_checks(),
        );
    }

    /// The pre-refactor schema used `Vec<ObjectID>` and `HashSet<String>`. On the wire
    /// (YAML lists), both serialize identically to the new `BTreeSet`-based schema.
    /// This test pins that backward compatibility down so a future refactor that
    /// changes the on-wire shape will fail loudly.
    #[test]
    fn yaml_pre_refactor_schema_still_parses() {
        // Hand-rolled YAML that matches what the old Vec/HashSet code would emit:
        // sequences for the list fields, with concrete entries (not the empty-list
        // serialization a fresh BTreeSet would produce).
        let yaml = r#"
            object-deny-list:
              - "0x0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a"
              - "0x0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"
            package-deny-list:
              - "0x0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"
            address-deny-list:
              - "0x0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d"
            package-publish-disabled: true
            package-upgrade-disabled: true
            shared-object-disabled: true
            user-transaction-disabled: true
            gasless-disabled: true
            receiving-objects-disabled: true
            zklogin-sig-disabled: true
            zklogin-disabled-providers:
              - Google
              - Apple
        "#;
        let cfg: TransactionDenyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.get_object_deny_set().len(), 2);
        assert_eq!(cfg.get_package_deny_set().len(), 1);
        assert_eq!(cfg.get_address_deny_set().len(), 1);
        assert_eq!(cfg.zklogin_disabled_providers().len(), 2);
        assert!(cfg.user_transaction_disabled());
        assert!(cfg.zklogin_sig_disabled());
    }

    /// `#[serde(flatten)]` plus the custom `dynamic_transaction_checks`
    /// (de)serializer is the trickiest serde combination in this struct. Verify
    /// they coexist correctly: a config with a populated Starlark program and
    /// flattened rule fields round-trips without one stomping on the other.
    #[test]
    fn yaml_round_trip_with_dynamic_transaction_checks() {
        // A trivially-valid Starlark program that always passes.
        let starlark =
            "def predicate(tx_data, tx_signatures, input_objects, receiving_objects):\n    pass\n"
                .to_string();
        let cfg = TransactionDenyConfigBuilder::new()
            .add_denied_object(obj(1))
            .disable_package_publish()
            .add_dynamic_transaction_checks(starlark)
            .expect("starlark should parse");
        let cfg = cfg.build();

        let yaml = serde_yaml::to_string(&cfg).expect("serialize");
        let parsed: TransactionDenyConfig = serde_yaml::from_str(&yaml).expect("deserialize");

        // Both flattened rule fields and the custom-serialized Starlark survive.
        assert_eq!(cfg.rules(), parsed.rules());
        assert!(parsed.has_dynamic_transaction_checks());
    }
}
