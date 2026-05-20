// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sui_types::base_types::{AuthorityName, ObjectID, SuiAddress};
pub use sui_types::transaction_deny_rules::TransactionDenyRules;

use crate::dynamic_transaction_signing_checks::{
    DynamicCheckRunnerContext, DynamicCheckRunnerError,
};

/// Configuration for activating recommended `TransactionDenyConfig` rules shared by
/// peers via consensus. The operator pre-defines named rulesets, each gated on a
/// stake threshold among an eligible set of validators; a "default" bucket governs any
/// rule elements that weren't pre-listed.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PeerDenySyncConfig {
    /// Pre-listed rulesets. Each activates only when eligible voting stake reaches
    /// its threshold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rulesets: Vec<SharedDenyRuleset>,

    /// Governs rule elements proposed by peers that aren't part of any pre-listed
    /// ruleset. Each proposed element is threshold-gated individually.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_threshold: Option<SharedDenyRuleThreshold>,

    #[serde(default)]
    pub broadcast_on_startup: bool,

    #[serde(default)]
    pub broadcast_on_epoch_change: bool,
}

/// A pre-listed ruleset becomes effective when validators holding at least
/// `threshold.stake_threshold_percent` of the eligible stake have each proposed a
/// superset of `rules`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SharedDenyRuleset {
    /// Operator-chosen identifier used in metrics.
    pub name: String,
    pub rules: TransactionDenyRules,
    #[serde(flatten)]
    pub threshold: SharedDenyRuleThreshold,
}

/// Eligibility + stake threshold criteria for activating a proposed deny-rule.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SharedDenyRuleThreshold {
    pub eligibility: ValidatorEligibility,
    /// Whole-number percent (0..=100) of eligible stake that must vote to activate.
    pub stake_threshold_percent: u16,
}

/// Which validators' proposals count toward a deny-rule activation's stake threshold.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ValidatorEligibility {
    /// Only the listed authorities are eligible.
    Allowlist(BTreeSet<AuthorityName>),
    /// All committee members except the listed authorities are eligible.
    Denylist(BTreeSet<AuthorityName>),
}

impl ValidatorEligibility {
    pub fn is_eligible(&self, name: &AuthorityName) -> bool {
        match self {
            ValidatorEligibility::Allowlist(set) => set.contains(name),
            ValidatorEligibility::Denylist(set) => !set.contains(name),
        }
    }
}

impl Default for ValidatorEligibility {
    fn default() -> Self {
        // An empty denylist makes every committee member eligible.
        ValidatorEligibility::Denylist(BTreeSet::new())
    }
}

impl PeerDenySyncConfig {
    /// Validate operator-provided settings. Called at manager construction.
    pub fn validate(&self) -> Result<(), String> {
        let mut names = BTreeSet::new();
        for ruleset in &self.rulesets {
            if ruleset.name.is_empty() {
                return Err("rulesets entry has an empty name".to_string());
            }
            if !names.insert(ruleset.name.as_str()) {
                return Err(format!("duplicate rulesets name: {}", ruleset.name));
            }
            if ruleset.rules.is_empty() {
                return Err(format!("rulesets entry {} has empty rules", ruleset.name));
            }
            ruleset.threshold.validate(&ruleset.name)?;
        }
        if let Some(default) = &self.default_threshold {
            default.validate("default_threshold")?;
        }
        Ok(())
    }
}

impl SharedDenyRuleThreshold {
    /// Validate this threshold's percent is within 0..=100. `label` is included in the
    /// error message to identify which threshold failed.
    pub fn validate(&self, label: &str) -> Result<(), String> {
        if self.stake_threshold_percent > 100 {
            return Err(format!(
                "{label}: stake_threshold_percent must be 0..=100, got {}",
                self.stake_threshold_percent,
            ));
        }
        Ok(())
    }
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

    /// Return a copy of this config with `rules` replaced, carrying
    /// `dynamic_transaction_checks` over verbatim (it is local-only and never shared).
    pub fn with_rules(&self, rules: TransactionDenyRules) -> Self {
        Self {
            rules,
            dynamic_transaction_checks: self.dynamic_transaction_checks.clone(),
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
    use sui_types::base_types::dbg_addr;

    #[test]
    fn with_rules_replaces_rules_and_keeps_dynamic_checks() {
        let starlark =
            "def predicate(tx_data, tx_signatures, input_objects, receiving_objects):\n    pass\n"
                .to_string();
        let local = TransactionDenyConfigBuilder::new()
            .add_denied_object(ObjectID::from_single_byte(1))
            .add_dynamic_transaction_checks(starlark)
            .expect("starlark should parse")
            .build();

        let mut new_rules = TransactionDenyRules::default();
        new_rules
            .object_deny_list
            .insert(ObjectID::from_single_byte(2));
        new_rules.user_transaction_disabled = true;

        let updated = local.with_rules(new_rules);

        assert!(
            !updated
                .get_object_deny_set()
                .contains(&ObjectID::from_single_byte(1))
        );
        assert!(
            updated
                .get_object_deny_set()
                .contains(&ObjectID::from_single_byte(2))
        );
        assert!(updated.user_transaction_disabled());
        // dynamic_transaction_checks is local-only and carried over verbatim.
        assert!(updated.has_dynamic_transaction_checks());
    }

    #[test]
    fn transaction_deny_config_yaml_preserves_existing_schema() {
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
    fn transaction_deny_config_yaml_round_trip() {
        let cfg = TransactionDenyConfigBuilder::new()
            .add_denied_object(ObjectID::from_single_byte(1))
            .add_denied_object(ObjectID::from_single_byte(2))
            .add_denied_package(ObjectID::from_single_byte(3))
            .add_denied_address(dbg_addr(4))
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
    fn transaction_deny_config_yaml_pre_refactor_schema_parses() {
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
    fn transaction_deny_config_yaml_round_trip_with_dynamic_checks() {
        // A trivially-valid Starlark program that always passes.
        let starlark =
            "def predicate(tx_data, tx_signatures, input_objects, receiving_objects):\n    pass\n"
                .to_string();
        let cfg = TransactionDenyConfigBuilder::new()
            .add_denied_object(ObjectID::from_single_byte(1))
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

    /// A populated `PeerDenySyncConfig` round-trips through YAML — pins down the
    /// `#[serde(flatten)]` on the ruleset threshold and the `ValidatorEligibility` enum
    /// representation, the two serde-fragile parts of the schema.
    #[test]
    fn peer_deny_sync_config_yaml_round_trip() {
        let rules = TransactionDenyRules {
            package_publish_disabled: true,
            object_deny_list: std::iter::once(ObjectID::from_single_byte(7)).collect(),
            ..Default::default()
        };
        let config = PeerDenySyncConfig {
            rulesets: vec![SharedDenyRuleset {
                name: "incident".to_string(),
                rules,
                threshold: SharedDenyRuleThreshold {
                    eligibility: ValidatorEligibility::Denylist(BTreeSet::new()),
                    stake_threshold_percent: 67,
                },
            }],
            default_threshold: Some(SharedDenyRuleThreshold {
                eligibility: ValidatorEligibility::Allowlist(BTreeSet::new()),
                stake_threshold_percent: 50,
            }),
            broadcast_on_startup: true,
            broadcast_on_epoch_change: false,
        };

        let yaml = serde_yaml::to_string(&config).expect("serialize");
        let parsed: PeerDenySyncConfig = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(config, parsed);
    }

    #[test]
    fn validate_rejects_malformed_rulesets() {
        let nonempty = || TransactionDenyRules {
            package_publish_disabled: true,
            ..Default::default()
        };
        let ruleset = |name: &str, rules: TransactionDenyRules, percent: u16| SharedDenyRuleset {
            name: name.to_string(),
            rules,
            threshold: SharedDenyRuleThreshold {
                eligibility: ValidatorEligibility::default(),
                stake_threshold_percent: percent,
            },
        };
        let config = |rulesets, default_threshold| PeerDenySyncConfig {
            rulesets,
            default_threshold,
            ..Default::default()
        };

        // A well-formed config validates.
        assert!(
            config(vec![ruleset("a", nonempty(), 50)], None)
                .validate()
                .is_ok()
        );
        // Empty name.
        assert!(
            config(vec![ruleset("", nonempty(), 50)], None)
                .validate()
                .is_err()
        );
        // Duplicate names.
        assert!(
            config(
                vec![
                    ruleset("dup", nonempty(), 50),
                    ruleset("dup", nonempty(), 50)
                ],
                None,
            )
            .validate()
            .is_err()
        );
        // Empty rules.
        assert!(
            config(
                vec![ruleset("a", TransactionDenyRules::default(), 50)],
                None
            )
            .validate()
            .is_err()
        );
        // Threshold above 100 on a pre-listed ruleset.
        assert!(
            config(vec![ruleset("a", nonempty(), 101)], None)
                .validate()
                .is_err()
        );
        // Threshold above 100 on the default bucket.
        assert!(
            config(
                vec![],
                Some(SharedDenyRuleThreshold {
                    eligibility: ValidatorEligibility::default(),
                    stake_threshold_percent: 101,
                }),
            )
            .validate()
            .is_err()
        );
    }
}
