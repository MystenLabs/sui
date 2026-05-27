// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Runtime manager for the `TransactionDenyConfig` used at voting time.
//!
//! Holds the operator's local configuration plus the latest deny-rule proposals
//! received from committee members over consensus (via
//! `ConsensusTransactionKind::UpdateTransactionDenyConfig`). The effective config is
//! recomputed by [`evaluate_deny_configs`]:
//!
//! - The operator's local `transaction_deny_config` is always applied unconditionally.
//! - Each operator-defined `SharedDenyRuleset` ("pre-listed" ruleset) activates only
//!   when validators holding at least its stake threshold of the *eligible* stake have
//!   each proposed a superset of its rules.
//! - A "default" bucket threshold-gates each individual proposed rule *element*
//!   (deny-list entry or boolean kill switch) that peers have proposed.
//!
//! The local validator is not a special case: its own broadcasts arrive back through
//! consensus and count as its vote, exactly like a peer's. A node that hasn't broadcast
//! (or has withdrawn) simply doesn't vote.

use crate::authority::authority_store_tables::AuthorityPerpetualTables;
use arc_swap::ArcSwap;
use itertools::Itertools;
use parking_lot::Mutex;
use prometheus::{
    IntGauge, IntGaugeVec, Registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use sui_config::transaction_deny_config::{
    PeerDenySyncConfig, TransactionDenyConfig, ValidatorEligibility,
};
use sui_types::base_types::AuthorityName;
use sui_types::base_types::ConciseableName;
use sui_types::committee::{Committee, StakeUnit, TOTAL_VOTING_POWER};
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages_consensus::SharedTransactionDenyConfig;
use sui_types::transaction_deny_rules::{DenyElement, DenyElementKind, TransactionDenyRules};
use tracing::{debug, info};
use typed_store::Map;

pub struct TransactionDenyConfigManager {
    self_authority: AuthorityName,
    /// The operator's local configuration at startup. Always applied unconditionally.
    local_config: Arc<TransactionDenyConfig>,
    /// Operator-defined pre-listed rulesets and the "default" bucket settings.
    sync_config: PeerDenySyncConfig,
    /// Latest accepted proposal per committee member. `BTreeMap` for deterministic
    /// iteration order during evaluation.
    peer_configs: Mutex<BTreeMap<AuthorityName, SharedTransactionDenyConfig>>,
    /// Current committee, used for per-validator stake and membership. Replaced on
    /// epoch change via `update_for_committee`.
    committee: ArcSwap<Committee>,
    /// Snapshot of the current effective config used by the voting hot path. Replaced
    /// atomically whenever evaluation produces different rules.
    effective_config: ArcSwap<TransactionDenyConfig>,
    /// Backing store for cross-restart durability of `peer_configs`.
    perpetual: Arc<AuthorityPerpetualTables>,
    metrics: TransactionDenyConfigMetrics,
}

impl TransactionDenyConfigManager {
    pub fn new(
        self_authority: AuthorityName,
        local_config: TransactionDenyConfig,
        sync_config: PeerDenySyncConfig,
        committee: Arc<Committee>,
        perpetual: Arc<AuthorityPerpetualTables>,
        registry: &Registry,
    ) -> SuiResult<Arc<Self>> {
        sync_config.validate().map_err(SuiError::from)?;
        let local_config = Arc::new(local_config);

        // Seed peer_configs from the perpetual store. Skip our own persisted
        // broadcast: after a restart the operator may have edited the local
        // transaction_deny_config, so a pre-restart self-broadcast could be stale.
        // Startup reconciliation (in sui-node) either re-broadcasts the current local
        // config or withdraws; until that lands we simply don't vote for ourselves
        // rather than vote a possibly-stale snapshot. Entries from validators no longer
        // in the committee are also skipped (and pruned from the DB by the next
        // `update_for_committee`).
        let mut peer_configs = BTreeMap::new();
        for entry in perpetual.shared_transaction_deny_configs.safe_iter() {
            let (authority, msg) = entry.expect("db error reading shared_transaction_deny_configs");
            if authority != self_authority && committee.authority_exists(&authority) {
                peer_configs.insert(authority, msg);
            }
        }

        let metrics = TransactionDenyConfigMetrics::new(registry);
        let evaluation = evaluate(&local_config, &sync_config, &peer_configs, &committee);
        let effective = local_config.with_rules(evaluation.effective_rules.clone());
        metrics.record(
            &local_config,
            active_proposal_count(&peer_configs),
            &evaluation,
        );

        info!(
            rulesets = sync_config.rulesets.len(),
            default_buckets = sync_config.default_buckets.len(),
            seeded_proposals = peer_configs.len(),
            "TransactionDenyConfigManager initialized",
        );

        Ok(Arc::new(Self {
            self_authority,
            local_config,
            sync_config,
            peer_configs: Mutex::new(peer_configs),
            committee: ArcSwap::from(committee),
            effective_config: ArcSwap::from_pointee(effective),
            perpetual,
            metrics,
        }))
    }

    /// Returns the local (operator-configured) deny config. Tests and the admin dump
    /// endpoint use this when they want the unmerged view.
    pub fn local(&self) -> &Arc<TransactionDenyConfig> {
        &self.local_config
    }

    /// Returns the merged effective deny config (local + threshold-gated peer rules).
    pub fn effective_config(&self) -> &ArcSwap<TransactionDenyConfig> {
        &self.effective_config
    }

    /// Snapshot of the currently-cached per-peer proposals.
    pub fn peer_configs_snapshot(&self) -> BTreeMap<AuthorityName, SharedTransactionDenyConfig> {
        self.peer_configs.lock().clone()
    }

    /// Evaluate the current voting state without mutating anything. Used by the admin
    /// dump endpoint for operator visibility.
    pub fn evaluate_status(&self) -> DenyConfigEvaluation {
        let committee = self.committee.load();
        let peer_configs = self.peer_configs.lock();
        evaluate(
            &self.local_config,
            &self.sync_config,
            &peer_configs,
            &committee,
        )
    }

    /// Returns true if the perpetual store holds an active (`Some`) broadcast from this
    /// node — i.e. an outstanding vote from before a restart.
    pub fn persisted_broadcast_is_active(&self) -> SuiResult<bool> {
        Ok(self
            .perpetual
            .shared_transaction_deny_configs
            .get(&self.self_authority)?
            .map(|msg| msg.rules().is_some())
            .unwrap_or(false))
    }

    /// Apply a peer-broadcast proposal received via consensus.
    /// See `apply_updates` for details.
    pub fn apply_update(&self, msg: SharedTransactionDenyConfig) -> SuiResult<()> {
        self.apply_updates(vec![msg])
    }

    /// Apply a batch of peer-broadcast proposals received in one consensus commit.
    /// Caller must have already verified sender authenticity.
    pub fn apply_updates(&self, msgs: Vec<SharedTransactionDenyConfig>) -> SuiResult<()> {
        if msgs.is_empty() {
            return Ok(());
        }
        let (evaluation, active_proposals) = {
            let mut peer_configs = self.peer_configs.lock();
            // Load the committee inside the critical section so this batch is validated
            // and evaluated against a single committee snapshot — even if
            // `update_for_committee` is concurrently swapping the committee.
            let committee = self.committee.load();
            let mut accepted = false;
            for msg in msgs {
                let authority = msg.authority();
                let generation = msg.generation();
                if !committee.authority_exists(&authority) {
                    info!(
                        authority = %authority.concise(),
                        "Dropping UpdateTransactionDenyConfig: sender not in committee",
                    );
                    continue;
                }
                if let Some(existing) = peer_configs.get(&authority)
                    && existing.generation() >= generation
                {
                    debug!(
                        authority = %authority.concise(),
                        new_generation = generation,
                        existing_generation = existing.generation(),
                        "Dropping UpdateTransactionDenyConfig: stale generation",
                    );
                    continue;
                }
                // Persist before swapping in-memory so a crash leaves state consistent.
                self.perpetual
                    .shared_transaction_deny_configs
                    .insert(&authority, &msg)?;
                peer_configs.insert(authority, msg);
                accepted = true;
                info!(
                    authority = %authority.concise(),
                    generation,
                    "Accepted UpdateTransactionDenyConfig from committee member",
                );
            }
            if !accepted {
                return Ok(());
            }
            (
                evaluate(
                    &self.local_config,
                    &self.sync_config,
                    &peer_configs,
                    &committee,
                ),
                active_proposal_count(&peer_configs),
            )
        };
        self.apply_evaluation(evaluation, active_proposals);
        Ok(())
    }

    /// Update the stored committee and prune proposals (in-memory + DB) from any
    /// authority that is no longer a member. Always recomputes the effective config,
    /// since the stake distribution may have shifted even with no departures. Called at
    /// epoch transitions and at startup.
    pub fn update_for_committee(&self, committee: Arc<Committee>) -> SuiResult<()> {
        let (evaluation, active_proposals) = {
            let mut peer_configs = self.peer_configs.lock();
            // Swap the committee inside the critical section so any concurrent
            // `apply_updates` either sees the old committee with the old peer set or
            // the new committee with the pruned peer set — never a mismatched pair.
            self.committee.store(committee.clone());
            let to_remove: Vec<AuthorityName> = peer_configs
                .keys()
                .filter(|name| !committee.authority_exists(name))
                .copied()
                .collect();
            for name in &to_remove {
                self.perpetual
                    .shared_transaction_deny_configs
                    .remove(name)?;
                peer_configs.remove(name);
            }
            if !to_remove.is_empty() {
                info!(
                    pruned = to_remove.len(),
                    "Pruned UpdateTransactionDenyConfig entries for peers no longer in committee",
                );
            }
            (
                evaluate(
                    &self.local_config,
                    &self.sync_config,
                    &peer_configs,
                    &committee,
                ),
                active_proposal_count(&peer_configs),
            )
        };
        self.apply_evaluation(evaluation, active_proposals);
        Ok(())
    }

    /// Allocate the next monotonic generation for an outgoing broadcast. Persists the
    /// returned value before any submission so a crash between allocate-and-send cannot
    /// reuse the generation.
    pub fn allocate_next_broadcast_generation(&self) -> SuiResult<u64> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_millis() as u64;
        let last = self
            .perpetual
            .last_broadcast_deny_generation
            .get(&())?
            .unwrap_or(0);
        let generation = now_ms.max(last + 1);
        self.perpetual
            .last_broadcast_deny_generation
            .insert(&(), &generation)?;
        Ok(generation)
    }

    /// Build a ready-to-submit `UpdateTransactionDenyConfig` consensus transaction along
    /// with the freshly-allocated generation. `Some(rules)` publishes a proposal;
    /// `None` withdraws. The generation is persisted before return so a crash mid-submit
    /// can never reuse it.
    pub fn build_share_consensus_tx(
        &self,
        rules: Option<TransactionDenyRules>,
    ) -> SuiResult<(sui_types::messages_consensus::ConsensusTransaction, u64)> {
        if let Some(rules) = &rules {
            rules.check_share_limits().map_err(SuiError::from)?;
        }
        let generation = self.allocate_next_broadcast_generation()?;
        let msg = SharedTransactionDenyConfig::V1(
            sui_types::messages_consensus::SharedTransactionDenyConfigV1 {
                authority: self.self_authority,
                generation,
                rules,
            },
        );
        let tx =
            sui_types::messages_consensus::ConsensusTransaction::new_update_transaction_deny_config(
                msg,
            );
        Ok((tx, generation))
    }

    /// Republish metrics and atomically swap in the new effective config.
    fn apply_evaluation(&self, evaluation: DenyConfigEvaluation, active_proposals: usize) {
        self.metrics
            .record(&self.local_config, active_proposals, &evaluation);
        let prev = self.effective_config.load();
        if &evaluation.effective_rules != prev.rules() {
            let new_effective = self.local_config.with_rules(evaluation.effective_rules);
            self.effective_config.store(Arc::new(new_effective));
        }
    }
}

/// Evaluate the current voting state from a manager's fields. A free function so `new`
/// (which has no `self` yet) and the instance methods share one wiring.
fn evaluate(
    local_config: &TransactionDenyConfig,
    sync_config: &PeerDenySyncConfig,
    peer_configs: &BTreeMap<AuthorityName, SharedTransactionDenyConfig>,
    committee: &Committee,
) -> DenyConfigEvaluation {
    let votes: BTreeMap<AuthorityName, &TransactionDenyRules> = peer_configs
        .iter()
        .filter(|(name, _)| committee.authority_exists(name))
        .filter_map(|(name, msg)| msg.rules().map(|rules| (*name, rules)))
        .collect();
    let stakes: BTreeMap<AuthorityName, StakeUnit> = committee
        .members()
        .map(|(name, stake)| (*name, *stake))
        .collect();
    evaluate_deny_configs(local_config.rules(), sync_config, &votes, &stakes)
}

/// Count of committee members with an accepted (`Some`) proposal.
fn active_proposal_count(
    peer_configs: &BTreeMap<AuthorityName, SharedTransactionDenyConfig>,
) -> usize {
    peer_configs
        .values()
        .filter(|m| m.rules().is_some())
        .count()
}

pub struct PrelistedRulesetStatus {
    pub name: String,
    pub stake_threshold_percent: u16,
    pub eligible_stake: StakeUnit,
    pub voted_stake: StakeUnit,
    pub voters: Vec<AuthorityName>,
    pub active: bool,
}

pub struct DefaultBucketStatus {
    pub name: String,
    pub element_kinds: BTreeSet<DenyElementKind>,
    pub stake_threshold_percent: u16,
    pub eligible_stake: StakeUnit,
    pub applied_elements: Vec<DenyElement>,
}

/// Result of evaluating the deny-config voting state.
pub struct DenyConfigEvaluation {
    /// The merged result — already includes the always-on local rules as its base, so
    /// this is the config to enforce, not a delta.
    pub effective_rules: TransactionDenyRules,
    pub prelisted: Vec<PrelistedRulesetStatus>,
    pub defaults: Vec<DefaultBucketStatus>,
}

/// Returns true if `voted` is at least `percent`% of `eligible` stake.
fn meets_threshold(voted: StakeUnit, eligible: StakeUnit, percent: u16) -> bool {
    if eligible == 0 {
        // Zero eligible stake never meets a threshold.
        return false;
    }
    // `voted / eligible >= percent / 100`, cross-multiplied to stay in integer math.
    // Both sides are bounded by `TOTAL_VOTING_POWER * 100` (1e6), well within `u64`.
    let voted_share = voted * 100;
    let required_share = StakeUnit::from(percent) * eligible;
    voted_share >= required_share
}

/// Total eligible stake under `eligibility`, the denominator for its threshold.
fn eligible_stake(
    eligibility: &ValidatorEligibility,
    stakes: &BTreeMap<AuthorityName, StakeUnit>,
) -> StakeUnit {
    match eligibility {
        ValidatorEligibility::Allowlist(set) => {
            set.iter().filter_map(|name| stakes.get(name)).sum()
        }
        ValidatorEligibility::Denylist(set) => {
            TOTAL_VOTING_POWER
                - set
                    .iter()
                    .filter_map(|name| stakes.get(name))
                    .sum::<StakeUnit>()
        }
    }
}

/// Pure evaluation of the deny-config voting state.
///
/// Each pre-listed ruleset is evaluated independently — a proposal counts as a vote for
/// *every* pre-listed ruleset whose rules it is a superset of (including
/// nested/overlapping rulesets). Each default bucket considers proposed elements
/// whose `DenyElementKind` it claims, threshold-gating them individually. Element kinds
/// not claimed by any bucket cannot be activated through the default path.
pub fn evaluate_deny_configs(
    local_rules: &TransactionDenyRules,
    sync_config: &PeerDenySyncConfig,
    votes: &BTreeMap<AuthorityName, &TransactionDenyRules>,
    stakes: &BTreeMap<AuthorityName, StakeUnit>,
) -> DenyConfigEvaluation {
    let prelisted: Vec<PrelistedRulesetStatus> = sync_config
        .rulesets
        .iter()
        .map(|ruleset| {
            let eligible_stake = eligible_stake(&ruleset.threshold.eligibility, stakes);
            // Collect the voter names and sum their stake in a single pass.
            let (voters, voted_stake): (Vec<AuthorityName>, StakeUnit) = votes
                .iter()
                .filter_map(|(name, rules)| {
                    let stake = stakes.get(name)?;
                    (ruleset.threshold.eligibility.is_eligible(name)
                        && rules.is_superset_of(&ruleset.rules))
                    .then_some((*name, *stake))
                })
                .fold((Vec::new(), 0), |(mut voters, total), (name, stake)| {
                    voters.push(name);
                    (voters, total + stake)
                });
            PrelistedRulesetStatus {
                name: ruleset.name.clone(),
                stake_threshold_percent: ruleset.threshold.stake_threshold_percent,
                eligible_stake,
                voted_stake,
                active: meets_threshold(
                    voted_stake,
                    eligible_stake,
                    ruleset.threshold.stake_threshold_percent,
                ),
                voters,
            }
        })
        .collect();

    // Build a kind → bucket-index lookup once. `validate()` guarantees each kind
    // appears in at most one bucket.
    let kind_to_bucket: BTreeMap<DenyElementKind, usize> = sync_config
        .default_buckets
        .iter()
        .enumerate()
        .flat_map(|(idx, bucket)| bucket.element_kinds.iter().map(move |k| (*k, idx)))
        .collect();

    // For each bucket, accumulate per-element stake from voters eligible for that
    // bucket. Initialize one entry per bucket so the parallel index lookup is safe.
    let mut per_bucket_element_stake: Vec<BTreeMap<DenyElement, StakeUnit>> =
        vec![BTreeMap::new(); sync_config.default_buckets.len()];
    for (name, rules) in votes {
        let Some(stake) = stakes.get(name) else {
            continue;
        };
        for element in rules.elements() {
            let Some(&bucket_idx) = kind_to_bucket.get(&element.kind()) else {
                continue;
            };
            let bucket = &sync_config.default_buckets[bucket_idx];
            if !bucket.threshold.eligibility.is_eligible(name) {
                continue;
            }
            *per_bucket_element_stake[bucket_idx]
                .entry(element)
                .or_default() += *stake;
        }
    }

    let defaults: Vec<DefaultBucketStatus> = sync_config
        .default_buckets
        .iter()
        .zip_eq(per_bucket_element_stake)
        .map(|(bucket, element_stake)| {
            let eligible_stake = eligible_stake(&bucket.threshold.eligibility, stakes);
            let applied_elements: Vec<DenyElement> = element_stake
                .into_iter()
                .filter(|(_, stake)| {
                    meets_threshold(
                        *stake,
                        eligible_stake,
                        bucket.threshold.stake_threshold_percent,
                    )
                })
                .map(|(element, _)| element)
                .collect();
            DefaultBucketStatus {
                name: bucket.name.clone(),
                element_kinds: bucket.element_kinds.clone(),
                stake_threshold_percent: bucket.threshold.stake_threshold_percent,
                eligible_stake,
                applied_elements,
            }
        })
        .collect();

    let mut effective_rules = local_rules.clone();
    for (ruleset, status) in sync_config.rulesets.iter().zip_eq(&prelisted) {
        if status.active {
            effective_rules.merge(&ruleset.rules);
        }
    }
    for default in &defaults {
        for element in &default.applied_elements {
            effective_rules.apply_element(element);
        }
    }

    DenyConfigEvaluation {
        effective_rules,
        prelisted,
        defaults,
    }
}

/// Gauges describing one view of a `TransactionDenyRules` — either the operator's local
/// rules or the post-merge effective rules. Both views publish the same metric shape.
struct DenyRulesGauges {
    user_transaction_disabled: IntGauge,
    shared_object_disabled: IntGauge,
    package_publish_disabled: IntGauge,
    package_upgrade_disabled: IntGauge,
    num_denied_objects: IntGauge,
    num_denied_packages: IntGauge,
    num_denied_addresses: IntGauge,
}

impl DenyRulesGauges {
    fn new(registry: &Registry, prefix: &str, layer_help: &str) -> Self {
        let gauge = |name: &str, help: String| {
            let g = IntGauge::new(format!("{prefix}_{name}"), help).unwrap();
            registry.register(Box::new(g.clone())).unwrap();
            g
        };
        Self {
            user_transaction_disabled: gauge(
                "user_transaction_disabled",
                format!("1 if user_transaction_disabled is set in the {layer_help}"),
            ),
            shared_object_disabled: gauge(
                "shared_object_disabled",
                format!("1 if shared_object_disabled is set in the {layer_help}"),
            ),
            package_publish_disabled: gauge(
                "package_publish_disabled",
                format!("1 if package_publish_disabled is set in the {layer_help}"),
            ),
            package_upgrade_disabled: gauge(
                "package_upgrade_disabled",
                format!("1 if package_upgrade_disabled is set in the {layer_help}"),
            ),
            num_denied_objects: gauge(
                "num_denied_objects",
                format!("Number of objects in the {layer_help} object_deny_list"),
            ),
            num_denied_packages: gauge(
                "num_denied_packages",
                format!("Number of packages in the {layer_help} package_deny_list"),
            ),
            num_denied_addresses: gauge(
                "num_denied_addresses",
                format!("Number of addresses in the {layer_help} address_deny_list"),
            ),
        }
    }

    fn set_from(&self, rules: &TransactionDenyRules) {
        self.user_transaction_disabled
            .set(rules.user_transaction_disabled as i64);
        self.shared_object_disabled
            .set(rules.shared_object_disabled as i64);
        self.package_publish_disabled
            .set(rules.package_publish_disabled as i64);
        self.package_upgrade_disabled
            .set(rules.package_upgrade_disabled as i64);
        self.num_denied_objects
            .set(rules.object_deny_list.len() as i64);
        self.num_denied_packages
            .set(rules.package_deny_list.len() as i64);
        self.num_denied_addresses
            .set(rules.address_deny_list.len() as i64);
    }
}

/// Prometheus metrics for the deny config manager. Distinguishes the operator's local
/// rules from the post-merge effective rules, and exposes per-pre-listed-config voting
/// status so dashboards can see "config X is one validator away from activating."
pub struct TransactionDenyConfigMetrics {
    local: DenyRulesGauges,
    effective: DenyRulesGauges,
    active_proposals: IntGauge,
    default_bucket_applied_elements: IntGaugeVec,
    default_bucket_eligible_stake: IntGaugeVec,
    shared_config_active: IntGaugeVec,
    shared_config_voted_bps: IntGaugeVec,
    shared_config_eligible_stake: IntGaugeVec,
}

impl TransactionDenyConfigMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            // The `tx_deny_config_*` prefix matches the legacy gauges that lived in
            // `sui_config::node_config_metrics`, so existing dashboards keep working.
            local: DenyRulesGauges::new(registry, "tx_deny_config", "local config"),
            effective: DenyRulesGauges::new(
                registry,
                "tx_deny_effective",
                "effective config (local + threshold-gated peer rules)",
            ),
            active_proposals: register_int_gauge_with_registry!(
                "tx_deny_active_proposals",
                "Number of committee members with an accepted (Some) deny-rule proposal",
                registry,
            )
            .unwrap(),
            default_bucket_applied_elements: register_int_gauge_vec_with_registry!(
                "tx_deny_default_bucket_applied_elements",
                "Number of rule elements activated via this default bucket",
                &["bucket"],
                registry,
            )
            .unwrap(),
            default_bucket_eligible_stake: register_int_gauge_vec_with_registry!(
                "tx_deny_default_bucket_eligible_stake",
                "Total eligible voting stake (denominator) for this default bucket",
                &["bucket"],
                registry,
            )
            .unwrap(),
            shared_config_active: register_int_gauge_vec_with_registry!(
                "tx_deny_ruleset_active",
                "1 if this pre-listed ruleset currently meets its stake threshold",
                &["ruleset"],
                registry,
            )
            .unwrap(),
            shared_config_voted_bps: register_int_gauge_vec_with_registry!(
                "tx_deny_ruleset_voted_bps",
                "Voted stake as basis points of eligible stake for this pre-listed ruleset",
                &["ruleset"],
                registry,
            )
            .unwrap(),
            shared_config_eligible_stake: register_int_gauge_vec_with_registry!(
                "tx_deny_ruleset_eligible_stake",
                "Total eligible voting stake for this pre-listed ruleset",
                &["ruleset"],
                registry,
            )
            .unwrap(),
        }
    }

    pub fn record(
        &self,
        local: &TransactionDenyConfig,
        active_proposals: usize,
        evaluation: &DenyConfigEvaluation,
    ) {
        self.local.set_from(local.rules());
        self.effective.set_from(&evaluation.effective_rules);
        self.active_proposals.set(active_proposals as i64);

        for status in &evaluation.prelisted {
            let labels = &[status.name.as_str()];
            self.shared_config_active
                .with_label_values(labels)
                .set(status.active as i64);
            self.shared_config_voted_bps
                .with_label_values(labels)
                .set(stake_bps(status.voted_stake, status.eligible_stake));
            self.shared_config_eligible_stake
                .with_label_values(labels)
                .set(status.eligible_stake as i64);
        }

        for default in &evaluation.defaults {
            let labels = &[default.name.as_str()];
            self.default_bucket_applied_elements
                .with_label_values(labels)
                .set(default.applied_elements.len() as i64);
            self.default_bucket_eligible_stake
                .with_label_values(labels)
                .set(default.eligible_stake as i64);
        }
    }
}

fn stake_bps(voted: StakeUnit, eligible: StakeUnit) -> i64 {
    if eligible == 0 {
        0
    } else {
        ((voted as u128 * 10_000) / eligible as u128) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastcrypto::traits::VerifyingKey;
    use sui_config::transaction_deny_config::{
        DefaultDenyBucket, SharedDenyRuleThreshold, SharedDenyRuleset,
        TransactionDenyConfigBuilder, ValidatorEligibility,
    };
    use sui_types::base_types::{ObjectID, dbg_addr};
    use sui_types::messages_consensus::SharedTransactionDenyConfigV1;

    fn fake_name(byte: u8) -> AuthorityName {
        AuthorityName::new([byte; sui_types::crypto::AuthorityPublicKey::LENGTH])
    }

    /// A `TransactionDenyRules` denying the objects identified by `bytes` — the common
    /// proposal/ruleset fixture across these tests.
    fn rules_with_objects(bytes: &[u8]) -> TransactionDenyRules {
        TransactionDenyRules {
            object_deny_list: bytes
                .iter()
                .map(|b| ObjectID::from_single_byte(*b))
                .collect(),
            ..Default::default()
        }
    }

    fn prelisted(
        name: &str,
        rules: TransactionDenyRules,
        eligibility: ValidatorEligibility,
        threshold: u16,
    ) -> SharedDenyRuleset {
        SharedDenyRuleset {
            name: name.to_string(),
            rules,
            threshold: SharedDenyRuleThreshold {
                eligibility,
                stake_threshold_percent: threshold,
            },
        }
    }

    fn default_bucket(
        name: &str,
        kinds: &[DenyElementKind],
        eligibility: ValidatorEligibility,
        threshold: u16,
    ) -> DefaultDenyBucket {
        DefaultDenyBucket {
            name: name.to_string(),
            element_kinds: kinds.iter().copied().collect(),
            threshold: SharedDenyRuleThreshold {
                eligibility,
                stake_threshold_percent: threshold,
            },
        }
    }

    // ===== Pure-evaluator tests (synthetic names + stakes, no committee) =====

    fn equal_stakes(names: &[AuthorityName]) -> BTreeMap<AuthorityName, StakeUnit> {
        names.iter().map(|n| (*n, 2500)).collect()
    }

    fn vote_refs(
        owned: &BTreeMap<AuthorityName, TransactionDenyRules>,
    ) -> BTreeMap<AuthorityName, &TransactionDenyRules> {
        owned.iter().map(|(name, rules)| (*name, rules)).collect()
    }

    #[test]
    fn meets_threshold_is_inclusive_and_handles_zero_eligible() {
        // Inclusive: exactly at the threshold counts.
        assert!(meets_threshold(5000, 10000, 50));
        assert!(!meets_threshold(4999, 10000, 50));
        // Zero eligible stake never meets a threshold.
        assert!(!meets_threshold(0, 0, 0));
    }

    #[test]
    fn evaluate_prelisted_threshold() {
        let names: Vec<_> = (1..=4).map(fake_name).collect();
        let stakes = equal_stakes(&names);
        let local = TransactionDenyConfigBuilder::new().build();
        let sync = PeerDenySyncConfig {
            rulesets: vec![prelisted(
                "c",
                rules_with_objects(&[1]),
                ValidatorEligibility::Allowlist(names.iter().copied().collect()),
                60,
            )],
            ..Default::default()
        };

        // 2/4 voters = 5000 stake = 50%, below the 60% threshold -> inactive.
        let votes: BTreeMap<_, _> = names[..2]
            .iter()
            .map(|n| (*n, rules_with_objects(&[1])))
            .collect();
        let eval = evaluate_deny_configs(local.rules(), &sync, &vote_refs(&votes), &stakes);
        assert!(!eval.prelisted[0].active);
        assert!(
            !eval
                .effective_rules
                .object_deny_list
                .contains(&ObjectID::from_single_byte(1))
        );

        // 3/4 voters = 7500 stake = 75% >= 60% -> active.
        let votes: BTreeMap<_, _> = names[..3]
            .iter()
            .map(|n| (*n, rules_with_objects(&[1])))
            .collect();
        let eval = evaluate_deny_configs(local.rules(), &sync, &vote_refs(&votes), &stakes);
        assert!(eval.prelisted[0].active);
        assert!(
            eval.effective_rules
                .object_deny_list
                .contains(&ObjectID::from_single_byte(1))
        );
    }

    #[test]
    fn evaluate_superset_votes_for_overlapping_and_nested_configs() {
        let names: Vec<_> = (1..=4).map(fake_name).collect();
        let stakes = equal_stakes(&names);
        let local = TransactionDenyConfigBuilder::new().build();
        let all = ValidatorEligibility::Allowlist(names.iter().copied().collect());
        let sync = PeerDenySyncConfig {
            rulesets: vec![
                // Partial overlap with `y`, and a subset of `z`.
                prelisted("x", rules_with_objects(&[1, 2]), all.clone(), 50),
                prelisted("y", rules_with_objects(&[2, 3]), all.clone(), 50),
                // Nesting: superset of `x`.
                prelisted("z", rules_with_objects(&[1, 2, 3]), all, 50),
            ],
            ..Default::default()
        };

        // 3/4 validators each propose {1,2,3} — a superset of all three configs.
        let votes: BTreeMap<_, _> = names[..3]
            .iter()
            .map(|n| (*n, rules_with_objects(&[1, 2, 3])))
            .collect();
        let eval = evaluate_deny_configs(local.rules(), &sync, &vote_refs(&votes), &stakes);
        assert!(eval.prelisted.iter().all(|p| p.active));

        // A proposal of just {1,2} votes for `x` only (superset of x, not y or z).
        let votes: BTreeMap<_, _> = names[..3]
            .iter()
            .map(|n| (*n, rules_with_objects(&[1, 2])))
            .collect();
        let eval = evaluate_deny_configs(local.rules(), &sync, &vote_refs(&votes), &stakes);
        assert!(eval.prelisted[0].active); // x
        assert!(!eval.prelisted[1].active); // y
        assert!(!eval.prelisted[2].active); // z
    }

    #[test]
    fn evaluate_allowlist_vs_denylist_eligibility() {
        let names: Vec<_> = (1..=4).map(fake_name).collect();
        let stakes = equal_stakes(&names);
        let local = TransactionDenyConfigBuilder::new().build();

        // Allowlist of only names[0..2]: eligible stake is 5000. Only names[1] votes
        // among the eligible (2500 = 50%), which is below the 60% threshold.
        let allow = PeerDenySyncConfig {
            rulesets: vec![prelisted(
                "c",
                rules_with_objects(&[1]),
                ValidatorEligibility::Allowlist(names[..2].iter().copied().collect()),
                60,
            )],
            ..Default::default()
        };
        // names[2] and names[3] vote but are not eligible -> no effect.
        let votes: BTreeMap<_, _> = names[1..]
            .iter()
            .map(|n| (*n, rules_with_objects(&[1])))
            .collect();
        let eval = evaluate_deny_configs(local.rules(), &allow, &vote_refs(&votes), &stakes);
        assert_eq!(eval.prelisted[0].eligible_stake, 5000);
        assert_eq!(eval.prelisted[0].voted_stake, 2500); // only names[1] is eligible
        assert!(!eval.prelisted[0].active);

        // Denylist of names[0]: eligible stake is 7500 (names[1..4]).
        let deny = PeerDenySyncConfig {
            rulesets: vec![prelisted(
                "c",
                rules_with_objects(&[1]),
                ValidatorEligibility::Denylist([names[0]].into_iter().collect()),
                50,
            )],
            ..Default::default()
        };
        let eval = evaluate_deny_configs(local.rules(), &deny, &vote_refs(&votes), &stakes);
        assert_eq!(eval.prelisted[0].eligible_stake, 7500);
        assert_eq!(eval.prelisted[0].voted_stake, 7500);
        assert!(eval.prelisted[0].active);
    }

    #[test]
    fn evaluate_default_per_element() {
        let names: Vec<_> = (1..=4).map(fake_name).collect();
        let stakes = equal_stakes(&names);
        let local = TransactionDenyConfigBuilder::new().build();
        let sync = PeerDenySyncConfig {
            default_buckets: vec![default_bucket(
                "objs",
                &[DenyElementKind::Object],
                ValidatorEligibility::default(),
                50,
            )],
            ..Default::default()
        };

        // Object 1 proposed by 3 validators (active); object 2 by only 1 (inactive).
        let mut votes = BTreeMap::new();
        votes.insert(names[0], rules_with_objects(&[1, 2]));
        votes.insert(names[1], rules_with_objects(&[1]));
        votes.insert(names[2], rules_with_objects(&[1]));
        let eval = evaluate_deny_configs(local.rules(), &sync, &vote_refs(&votes), &stakes);
        assert!(
            eval.effective_rules
                .object_deny_list
                .contains(&ObjectID::from_single_byte(1))
        );
        assert!(
            !eval
                .effective_rules
                .object_deny_list
                .contains(&ObjectID::from_single_byte(2))
        );
        assert_eq!(eval.defaults.len(), 1);
        assert_eq!(
            eval.defaults[0].applied_elements,
            vec![DenyElement::Object(ObjectID::from_single_byte(1))]
        );
    }

    #[test]
    fn evaluate_element_counts_for_both_prelisted_and_default() {
        let names: Vec<_> = (1..=4).map(fake_name).collect();
        let stakes = equal_stakes(&names);
        let local = TransactionDenyConfigBuilder::new().build();
        let all = ValidatorEligibility::Allowlist(names.iter().copied().collect());
        // Pre-listed ruleset `c` requires 90% (unreachable here); default requires 50%.
        let sync = PeerDenySyncConfig {
            rulesets: vec![prelisted("c", rules_with_objects(&[1]), all, 90)],
            default_buckets: vec![default_bucket(
                "objs",
                &[DenyElementKind::Object],
                ValidatorEligibility::default(),
                50,
            )],
            ..Default::default()
        };
        // 3/4 propose object 1: pre-listed ruleset `c` stays inactive (below 90%),
        // but the default bucket still applies it, counting it there independently.
        let votes: BTreeMap<_, _> = names[..3]
            .iter()
            .map(|n| (*n, rules_with_objects(&[1])))
            .collect();
        let eval = evaluate_deny_configs(local.rules(), &sync, &vote_refs(&votes), &stakes);
        assert!(!eval.prelisted[0].active);
        assert!(
            eval.effective_rules
                .object_deny_list
                .contains(&ObjectID::from_single_byte(1))
        );
    }

    #[test]
    fn evaluate_default_buckets_segregate_by_kind() {
        // Two buckets with different thresholds. All four validators vote both an
        // object and `UserTransactionDisabled`. The object kind is in bucket A at
        // 50% (≤ 75% achieved, activates); the kill-switch kind is in bucket B at
        // 90% (> 75% achieved, does not activate).
        let names: Vec<_> = (1..=4).map(fake_name).collect();
        let stakes = equal_stakes(&names);
        let local = TransactionDenyConfigBuilder::new().build();
        let sync = PeerDenySyncConfig {
            default_buckets: vec![
                default_bucket(
                    "objs",
                    &[DenyElementKind::Object],
                    ValidatorEligibility::default(),
                    50,
                ),
                default_bucket(
                    "kill-switches",
                    &[DenyElementKind::UserTransactionDisabled],
                    ValidatorEligibility::default(),
                    90,
                ),
            ],
            ..Default::default()
        };

        let proposal = TransactionDenyRules {
            object_deny_list: [ObjectID::from_single_byte(1)].into_iter().collect(),
            user_transaction_disabled: true,
            ..Default::default()
        };
        let votes: BTreeMap<_, _> = names[..3].iter().map(|n| (*n, proposal.clone())).collect();
        let eval = evaluate_deny_configs(local.rules(), &sync, &vote_refs(&votes), &stakes);

        assert!(
            eval.effective_rules
                .object_deny_list
                .contains(&ObjectID::from_single_byte(1))
        );
        assert!(!eval.effective_rules.user_transaction_disabled);
        assert_eq!(eval.defaults.len(), 2);
        assert_eq!(
            eval.defaults[0].applied_elements,
            vec![DenyElement::Object(ObjectID::from_single_byte(1))],
        );
        assert!(eval.defaults[1].applied_elements.is_empty());
    }

    #[test]
    fn evaluate_default_unconfigured_kind_never_applies() {
        // The lone default bucket covers `Object` only. `UserTransactionDisabled` is
        // claimed by no bucket, so even unanimous votes leave it inactive.
        let names: Vec<_> = (1..=4).map(fake_name).collect();
        let stakes = equal_stakes(&names);
        let local = TransactionDenyConfigBuilder::new().build();
        let sync = PeerDenySyncConfig {
            default_buckets: vec![default_bucket(
                "objs",
                &[DenyElementKind::Object],
                ValidatorEligibility::default(),
                50,
            )],
            ..Default::default()
        };

        let proposal = TransactionDenyRules {
            user_transaction_disabled: true,
            ..Default::default()
        };
        let votes: BTreeMap<_, _> = names.iter().map(|n| (*n, proposal.clone())).collect();
        let eval = evaluate_deny_configs(local.rules(), &sync, &vote_refs(&votes), &stakes);

        assert!(!eval.effective_rules.user_transaction_disabled);
        assert!(eval.defaults[0].applied_elements.is_empty());
    }

    #[test]
    fn evaluate_default_bucket_eligibility_is_per_bucket() {
        // Two buckets covering disjoint kinds with disjoint eligibility sets.
        // names[0..2] are eligible for `objs`; names[2..4] for `kill-switches`.
        // A voter eligible for one bucket but not the other contributes only to the
        // bucket that lists them.
        let names: Vec<_> = (1..=4).map(fake_name).collect();
        let stakes = equal_stakes(&names);
        let local = TransactionDenyConfigBuilder::new().build();
        let objs_allowlist = ValidatorEligibility::Allowlist(names[..2].iter().copied().collect());
        let kill_allowlist = ValidatorEligibility::Allowlist(names[2..].iter().copied().collect());
        let sync = PeerDenySyncConfig {
            default_buckets: vec![
                default_bucket("objs", &[DenyElementKind::Object], objs_allowlist, 50),
                default_bucket(
                    "kill-switches",
                    &[DenyElementKind::UserTransactionDisabled],
                    kill_allowlist,
                    50,
                ),
            ],
            ..Default::default()
        };

        // Every validator proposes both an object and `UserTransactionDisabled`.
        let proposal = TransactionDenyRules {
            object_deny_list: [ObjectID::from_single_byte(1)].into_iter().collect(),
            user_transaction_disabled: true,
            ..Default::default()
        };
        let votes: BTreeMap<_, _> = names.iter().map(|n| (*n, proposal.clone())).collect();
        let eval = evaluate_deny_configs(local.rules(), &sync, &vote_refs(&votes), &stakes);

        // Bucket A: only names[0..2] (5000 stake = 100% of eligible) vote for the
        // object — activates.
        assert_eq!(eval.defaults[0].eligible_stake, 5000);
        assert_eq!(
            eval.defaults[0].applied_elements,
            vec![DenyElement::Object(ObjectID::from_single_byte(1))],
        );
        // Bucket B: only names[2..4] (5000 stake = 100% of eligible) vote for the
        // kill switch — activates.
        assert_eq!(eval.defaults[1].eligible_stake, 5000);
        assert_eq!(
            eval.defaults[1].applied_elements,
            vec![DenyElement::UserTransactionDisabled],
        );
        assert!(
            eval.effective_rules
                .object_deny_list
                .contains(&ObjectID::from_single_byte(1))
        );
        assert!(eval.effective_rules.user_transaction_disabled);
    }

    #[test]
    fn evaluate_local_rules_always_applied() {
        let names: Vec<_> = (1..=4).map(fake_name).collect();
        let stakes = equal_stakes(&names);
        let local = TransactionDenyConfigBuilder::new()
            .add_denied_address(dbg_addr(9))
            .build();
        let sync = PeerDenySyncConfig::default();
        let eval = evaluate_deny_configs(local.rules(), &sync, &BTreeMap::new(), &stakes);
        assert!(
            eval.effective_rules
                .address_deny_list
                .contains(&dbg_addr(9))
        );
    }

    // ===== Manager tests (real committee so authority names are valid) =====

    fn test_committee(size: usize) -> (Arc<Committee>, Vec<AuthorityName>) {
        let (committee, _kps) =
            Committee::new_simple_test_committee_with_normalized_voting_power(vec![1; size]);
        let names: Vec<AuthorityName> = committee.names().copied().collect();
        (Arc::new(committee), names)
    }

    fn open_perpetual() -> (tempfile::TempDir, Arc<AuthorityPerpetualTables>) {
        let dir = tempfile::tempdir().unwrap();
        let perpetual = Arc::new(AuthorityPerpetualTables::open(dir.path(), None, None));
        (dir, perpetual)
    }

    fn manager_with(
        self_authority: AuthorityName,
        local: TransactionDenyConfig,
        sync_config: PeerDenySyncConfig,
        committee: Arc<Committee>,
    ) -> (Arc<TransactionDenyConfigManager>, tempfile::TempDir) {
        let (dir, perpetual) = open_perpetual();
        let registry = Registry::new();
        let manager = TransactionDenyConfigManager::new(
            self_authority,
            local,
            sync_config,
            committee,
            perpetual,
            &registry,
        )
        .unwrap();
        (manager, dir)
    }

    fn make_msg(
        authority: AuthorityName,
        generation: u64,
        rules: Option<TransactionDenyRules>,
    ) -> SharedTransactionDenyConfig {
        SharedTransactionDenyConfig::V1(SharedTransactionDenyConfigV1 {
            authority,
            generation,
            rules,
        })
    }

    /// A pre-listed ruleset eligible for the whole committee at a 50% threshold.
    fn sync_with_prelisted(committee: &Committee) -> PeerDenySyncConfig {
        PeerDenySyncConfig {
            rulesets: vec![prelisted(
                "c",
                rules_with_objects(&[1]),
                ValidatorEligibility::Allowlist(committee.names().copied().collect()),
                50,
            )],
            ..Default::default()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn apply_update_rejects_non_committee_sender() {
        let (committee, names) = test_committee(4);
        let local = TransactionDenyConfigBuilder::new().build();
        let (manager, _dir) =
            manager_with(names[0], local, sync_with_prelisted(&committee), committee);

        let outsider = fake_name(200);
        manager
            .apply_update(make_msg(outsider, 1, Some(rules_with_objects(&[1]))))
            .unwrap();
        assert!(manager.peer_configs_snapshot().is_empty());
    }

    /// This node's own broadcast loops back through consensus and is accepted like any
    /// peer's — it is no longer dropped as a self-loopback.
    #[tokio::test(flavor = "multi_thread")]
    async fn apply_update_accepts_self_broadcast() {
        let (committee, names) = test_committee(4);
        let local = TransactionDenyConfigBuilder::new().build();
        let (manager, _dir) =
            manager_with(names[0], local, sync_with_prelisted(&committee), committee);

        manager
            .apply_update(make_msg(names[0], 1, Some(rules_with_objects(&[1]))))
            .unwrap();
        let snapshot = manager.peer_configs_snapshot();
        assert!(
            snapshot.contains_key(&names[0]),
            "self-broadcast should be accepted into peer_configs",
        );
        // It also counts as a vote: self is one voter for ruleset `c`.
        let status = manager.evaluate_status();
        assert_eq!(status.prelisted[0].voters, vec![names[0]]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn apply_update_ignores_stale_generations() {
        let (committee, names) = test_committee(4);
        let local = TransactionDenyConfigBuilder::new().build();
        let (manager, _dir) =
            manager_with(names[0], local, sync_with_prelisted(&committee), committee);

        manager
            .apply_update(make_msg(names[1], 100, Some(rules_with_objects(&[1]))))
            .unwrap();
        // Older generation must be dropped even though it carries different rules.
        manager
            .apply_update(make_msg(names[1], 50, Some(rules_with_objects(&[2]))))
            .unwrap();
        let snapshot = manager.peer_configs_snapshot();
        assert_eq!(snapshot.get(&names[1]).unwrap().generation(), 100);
    }

    /// On construction the manager seeds `peer_configs` from the perpetual store,
    /// keeping committee peers but skipping (a) its own persisted broadcast — which a
    /// since-edited local config could have made stale — and (b) authorities no longer
    /// in the committee.
    #[tokio::test(flavor = "multi_thread")]
    async fn new_seeds_committee_peers_only() {
        let (committee, names) = test_committee(4);
        let (dir, perpetual) = open_perpetual();
        // Self, a committee peer, and an outsider all have a persisted broadcast.
        for authority in [names[0], names[1], fake_name(200)] {
            perpetual
                .shared_transaction_deny_configs
                .insert(
                    &authority,
                    &make_msg(authority, 5, Some(rules_with_objects(&[1]))),
                )
                .unwrap();
        }

        let registry = Registry::new();
        let manager = TransactionDenyConfigManager::new(
            names[0],
            TransactionDenyConfigBuilder::new().build(),
            sync_with_prelisted(&committee),
            committee,
            perpetual,
            &registry,
        )
        .unwrap();

        let snapshot = manager.peer_configs_snapshot();
        assert!(
            snapshot.contains_key(&names[1]),
            "committee peer should be seeded",
        );
        assert!(
            !snapshot.contains_key(&names[0]),
            "own persisted broadcast must not be seeded",
        );
        assert!(
            !snapshot.contains_key(&fake_name(200)),
            "non-committee authority must not be seeded",
        );
        drop(dir);
    }

    /// `persisted_broadcast_is_active` reflects the DB regardless of in-memory seeding.
    #[tokio::test(flavor = "multi_thread")]
    async fn persisted_broadcast_is_active_reads_db() {
        let (committee, names) = test_committee(4);
        let (dir, perpetual) = open_perpetual();
        perpetual
            .shared_transaction_deny_configs
            .insert(
                &names[0],
                &make_msg(names[0], 5, Some(rules_with_objects(&[1]))),
            )
            .unwrap();
        let registry = Registry::new();
        let manager = TransactionDenyConfigManager::new(
            names[0],
            TransactionDenyConfigBuilder::new().build(),
            sync_with_prelisted(&committee),
            committee,
            perpetual,
            &registry,
        )
        .unwrap();

        // An active (`Some`) persisted self-broadcast is reported even though it was
        // not seeded into `peer_configs`.
        assert!(manager.persisted_broadcast_is_active().unwrap());

        // A withdrawal (`None`) is not active.
        manager
            .perpetual
            .shared_transaction_deny_configs
            .insert(&names[0], &make_msg(names[0], 6, None))
            .unwrap();
        assert!(!manager.persisted_broadcast_is_active().unwrap());
        drop(dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_for_committee_prunes_departed_peer() {
        let (committee, names) = test_committee(4);
        let local = TransactionDenyConfigBuilder::new().build();
        let (manager, _dir) = manager_with(
            names[0],
            local,
            sync_with_prelisted(&committee),
            committee.clone(),
        );

        manager
            .apply_update(make_msg(names[1], 1, Some(rules_with_objects(&[1]))))
            .unwrap();
        assert!(manager.peer_configs_snapshot().contains_key(&names[1]));

        // New committee drops names[1]; the manager prunes its cached entry.
        let remaining: BTreeMap<AuthorityName, StakeUnit> = committee
            .members()
            .filter(|&&(name, _)| name != names[1])
            .map(|(name, _)| (*name, 1))
            .collect();
        let new_committee = Arc::new(Committee::new_for_testing_with_normalized_voting_power(
            committee.epoch(),
            remaining,
        ));
        manager.update_for_committee(new_committee).unwrap();
        assert!(!manager.peer_configs_snapshot().contains_key(&names[1]));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn withdrawal_clears_contribution_but_keeps_row() {
        let (committee, names) = test_committee(4);
        let local = TransactionDenyConfigBuilder::new().build();
        let sync = PeerDenySyncConfig {
            default_buckets: vec![default_bucket(
                "objs",
                &[DenyElementKind::Object],
                ValidatorEligibility::default(),
                10,
            )],
            ..Default::default()
        };
        let (manager, _dir) = manager_with(names[0], local, sync, committee);

        manager
            .apply_update(make_msg(names[1], 10, Some(rules_with_objects(&[1]))))
            .unwrap();
        assert!(
            manager
                .effective_config()
                .load()
                .get_object_deny_set()
                .contains(&ObjectID::from_single_byte(1))
        );

        manager.apply_update(make_msg(names[1], 11, None)).unwrap();
        assert!(
            manager
                .effective_config()
                .load()
                .get_object_deny_set()
                .is_empty()
        );
        // Row kept (rules=None) so a delayed older `Some` can't resurrect it.
        let snapshot = manager.peer_configs_snapshot();
        let entry = snapshot.get(&names[1]).unwrap();
        assert_eq!(entry.generation(), 11);
        assert!(entry.rules().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn allocate_next_broadcast_generation_is_monotonic() {
        let (committee, names) = test_committee(4);
        let local = TransactionDenyConfigBuilder::new().build();
        let (manager, _dir) =
            manager_with(names[0], local, PeerDenySyncConfig::default(), committee);

        let g1 = manager.allocate_next_broadcast_generation().unwrap();
        let g2 = manager.allocate_next_broadcast_generation().unwrap();
        let g3 = manager.allocate_next_broadcast_generation().unwrap();
        assert!(g2 > g1);
        assert!(g3 > g2);
    }

    /// `build_share_consensus_tx` is the single chokepoint for outgoing broadcasts: it
    /// rejects rules the consensus validator would otherwise reject downstream.
    #[tokio::test(flavor = "multi_thread")]
    async fn build_share_consensus_tx_enforces_share_limit() {
        let (committee, names) = test_committee(4);
        let (manager, _dir) = manager_with(
            names[0],
            TransactionDenyConfigBuilder::new().build(),
            PeerDenySyncConfig::default(),
            committee,
        );

        // Within-limit rules and a withdrawal both build fine.
        assert!(
            manager
                .build_share_consensus_tx(Some(rules_with_objects(&[1])))
                .is_ok()
        );
        assert!(manager.build_share_consensus_tx(None).is_ok());

        // A zkLogin provider name past the per-string limit makes the rules
        // unshareable, so the build is rejected.
        let oversized = TransactionDenyRules {
            zklogin_disabled_providers: std::iter::once(
                "x".repeat(TransactionDenyRules::MAX_ZKLOGIN_PROVIDER_LENGTH + 1),
            )
            .collect(),
            ..Default::default()
        };
        assert!(manager.build_share_consensus_tx(Some(oversized)).is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_sync_config_is_rejected() {
        let (committee, names) = test_committee(4);
        let (_dir, perpetual) = open_perpetual();
        let registry = Registry::new();
        // Threshold out of range.
        let bad = PeerDenySyncConfig {
            rulesets: vec![prelisted(
                "c",
                rules_with_objects(&[1]),
                ValidatorEligibility::default(),
                150,
            )],
            ..Default::default()
        };
        assert!(
            TransactionDenyConfigManager::new(
                names[0],
                TransactionDenyConfigBuilder::new().build(),
                bad,
                committee,
                perpetual,
                &registry,
            )
            .is_err()
        );
    }
}
