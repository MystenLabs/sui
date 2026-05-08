// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Runtime manager for the `TransactionDenyConfig` used at voting time.
//!
//! Holds the operator's local configuration plus the latest accepted recommendations
//! from each allowlisted peer (received over consensus via
//! `ConsensusTransactionKind::UpdateTransactionDenyConfig`).

use crate::authority::authority_store_tables::AuthorityPerpetualTables;
use arc_swap::ArcSwap;
use parking_lot::Mutex;
use prometheus::{IntGauge, IntGaugeVec, Registry, register_int_gauge_vec_with_registry};
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_config::transaction_deny_config::{PeerDenySyncConfig, TransactionDenyConfig};
use sui_types::base_types::AuthorityName;
use sui_types::base_types::ConciseableName;
use sui_types::committee::Committee;
use sui_types::error::SuiResult;
use sui_types::messages_consensus::SharedTransactionDenyConfig;
use tracing::{debug, info, warn};
use typed_store::Map;

pub struct TransactionDenyConfigManager {
    self_authority: AuthorityName,
    /// The operator's local configuration at startup.
    local_config: Arc<TransactionDenyConfig>,
    /// Authorities whose updates we accept.
    allowlist: PeerDenySyncConfig,
    /// Latest accepted message per peer authority. `BTreeMap` for deterministic
    /// iteration order in `compute_effective`.
    peer_configs: Mutex<BTreeMap<AuthorityName, SharedTransactionDenyConfig>>,
    /// Snapshot of current effective config used by the voting hot path. Replaced
    /// atomically on each accepted update or committee-change pruning.
    effective_config: ArcSwap<TransactionDenyConfig>,
    /// Backing store for cross-restart durability of `peer_configs`.
    perpetual: Arc<AuthorityPerpetualTables>,
    metrics: TransactionDenyConfigMetrics,
}

impl TransactionDenyConfigManager {
    pub fn new(
        self_authority: AuthorityName,
        local_config: TransactionDenyConfig,
        allowlist: PeerDenySyncConfig,
        perpetual: Arc<AuthorityPerpetualTables>,
        registry: &Registry,
    ) -> SuiResult<Arc<Self>> {
        let local_config = Arc::new(local_config);

        // Seed peer_configs from the perpetual store, filtering by the current
        // allowlist. Entries from peers that the operator has since removed from the
        // allowlist remain in the DB (in case the operator re-adds them later) but
        // are not considered for voting.
        let mut peer_configs = BTreeMap::new();
        for entry in perpetual.shared_transaction_deny_configs.safe_iter() {
            let (authority, msg) = entry.expect("db error reading shared_transaction_deny_configs");
            if allowlist.peer_allowlist.contains(&authority) {
                peer_configs.insert(authority, msg);
            }
        }

        let effective = TransactionDenyConfig::from_local_and_peers(
            &local_config,
            peer_configs.values().filter_map(extract_rules),
        );
        let metrics = TransactionDenyConfigMetrics::new(registry);
        metrics.record(&local_config, &peer_configs, &effective);

        info!(
            allowlist_size = allowlist.peer_allowlist.len(),
            seeded_peer_recommendations = peer_configs.len(),
            "TransactionDenyConfigManager initialized",
        );

        Ok(Arc::new(Self {
            self_authority,
            local_config,
            allowlist,
            peer_configs: Mutex::new(peer_configs),
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

    /// Returns the merged effective deny config (local + allowlisted peer
    /// recommendations).
    pub fn effective_config(&self) -> &ArcSwap<TransactionDenyConfig> {
        &self.effective_config
    }

    /// Snapshot of the currently-cached per-peer recommendations.
    pub fn peer_configs_snapshot(&self) -> BTreeMap<AuthorityName, SharedTransactionDenyConfig> {
        self.peer_configs.lock().clone()
    }

    /// Apply a peer-broadcast update received via consensus. Caller must
    /// have already verified sender authenticity.
    ///
    /// Drops the update if the sender isn't on this node's allowlist or if the
    /// generation doesn't strictly advance the most recently accepted one for that
    /// peer (replay / out-of-order protection).
    pub fn apply_update(&self, msg: SharedTransactionDenyConfig) -> SuiResult<()> {
        let authority = msg.authority();
        let generation = msg.generation();
        if authority == self.self_authority {
            debug!(generation, "Dropping UpdateTransactionDenyConfig from self");
            return Ok(());
        }
        if !self.allowlist.peer_allowlist.contains(&authority) {
            debug!(
                authority = %authority.concise(),
                "Dropping UpdateTransactionDenyConfig: peer not allowlisted",
            );
            return Ok(());
        }

        let (new_effective, peers_snapshot) = {
            let mut peer_configs = self.peer_configs.lock();
            if let Some(existing) = peer_configs.get(&authority)
                && existing.generation() >= generation
            {
                debug!(
                    authority = %authority.concise(),
                    new_generation = generation,
                    existing_generation = existing.generation(),
                    "Dropping UpdateTransactionDenyConfig: stale generation",
                );
                return Ok(());
            }
            // Persist before swapping in-memory so a crash leaves state consistent.
            self.perpetual
                .shared_transaction_deny_configs
                .insert(&authority, &msg)?;
            peer_configs.insert(authority, msg);
            (self.compute_effective(&peer_configs), peer_configs.clone())
        };

        self.update_effective(new_effective, &peers_snapshot);
        info!(
            authority = %authority.concise(),
            generation,
            "Accepted UpdateTransactionDenyConfig from allowlisted peer",
        );
        Ok(())
    }

    /// Drop entries (in-memory + DB) for any authority that is no longer in the active
    /// committee. Called at epoch transitions and at startup.
    pub fn prune_for_committee(&self, committee: &Committee) -> SuiResult<()> {
        let (new_effective, peers_snapshot, removed) = {
            let mut peer_configs = self.peer_configs.lock();
            let to_remove: Vec<AuthorityName> = peer_configs
                .keys()
                .filter(|name| !committee.authority_exists(name))
                .copied()
                .collect();
            if to_remove.is_empty() {
                return Ok(());
            }
            for name in &to_remove {
                self.perpetual
                    .shared_transaction_deny_configs
                    .remove(name)?;
                peer_configs.remove(name);
            }
            (
                self.compute_effective(&peer_configs),
                peer_configs.clone(),
                to_remove,
            )
        };

        self.update_effective(new_effective, &peers_snapshot);
        warn!(
            pruned = removed.len(),
            "Pruned UpdateTransactionDenyConfig entries for peers no longer in committee",
        );
        Ok(())
    }

    /// Republish metrics and atomically swap in the new effective config.
    fn update_effective(
        &self,
        new_effective: TransactionDenyConfig,
        peers_snapshot: &BTreeMap<AuthorityName, SharedTransactionDenyConfig>,
    ) {
        // Metrics are updated unconditionally because the per-peer gauges
        // (`peer_num_denied_*`, `peer_last_update_generation`, etc.) can move even
        // when the merged effective rules don't — e.g. a peer adds an entry that was
        // already in another peer's list. The `ArcSwap` store, on the other hand, is
        // skipped on no-op so we don't wake up voting-path readers for nothing.
        self.metrics
            .record(&self.local_config, peers_snapshot, &new_effective);
        let prev = self.effective_config.load();
        if new_effective.rules() != prev.rules() {
            self.effective_config.store(Arc::new(new_effective));
        }
    }

    /// Allocate the next monotonic generation for an outgoing broadcast. Persists the
    /// returned value before any submission so a crash between allocate-and-send
    /// cannot reuse the generation.
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

    /// Build a ready-to-submit `UpdateTransactionDenyConfig` consensus transaction
    /// along with the freshly-allocated generation. `Some(rules)` publishes a
    /// recommendation; `None` withdraws. The generation is persisted before return so
    /// a crash mid-submit can never reuse it.
    pub fn build_share_consensus_tx(
        &self,
        authority: AuthorityName,
        rules: Option<sui_types::transaction_deny_rules::TransactionDenyRules>,
    ) -> SuiResult<(sui_types::messages_consensus::ConsensusTransaction, u64)> {
        let generation = self.allocate_next_broadcast_generation()?;
        let msg = SharedTransactionDenyConfig::V1(
            sui_types::messages_consensus::SharedTransactionDenyConfigV1 {
                authority,
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

    fn compute_effective(
        &self,
        peer_configs: &BTreeMap<AuthorityName, SharedTransactionDenyConfig>,
    ) -> TransactionDenyConfig {
        TransactionDenyConfig::from_local_and_peers(
            &self.local_config,
            peer_configs.values().filter_map(extract_rules),
        )
    }
}

fn extract_rules(
    msg: &SharedTransactionDenyConfig,
) -> Option<&sui_types::transaction_deny_rules::TransactionDenyRules> {
    msg.rules()
}

/// Gauges describing one view of a `TransactionDenyConfig` — either the operator's
/// local config or the post-merge effective config. Both views publish the same
/// metric shape, so we register and update them through a shared struct rather than
/// duplicating each metric and each `.set()` call.
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

    fn set_from(&self, cfg: &TransactionDenyConfig) {
        self.user_transaction_disabled
            .set(cfg.user_transaction_disabled() as i64);
        self.shared_object_disabled
            .set(cfg.shared_object_disabled() as i64);
        self.package_publish_disabled
            .set(cfg.package_publish_disabled() as i64);
        self.package_upgrade_disabled
            .set(cfg.package_upgrade_disabled() as i64);
        self.num_denied_objects
            .set(cfg.get_object_deny_set().len() as i64);
        self.num_denied_packages
            .set(cfg.get_package_deny_set().len() as i64);
        self.num_denied_addresses
            .set(cfg.get_address_deny_set().len() as i64);
    }
}

/// Prometheus metrics for the deny config manager. Distinguishes locally-configured
/// rules from the post-merge effective rules so dashboards can detect "we are blocking
/// X because peer Y recommended it."
pub struct TransactionDenyConfigMetrics {
    local: DenyRulesGauges,
    effective: DenyRulesGauges,
    active_peer_recommendations: IntGauge,
    peer_num_denied_objects: IntGaugeVec,
    peer_num_denied_packages: IntGaugeVec,
    peer_num_denied_addresses: IntGaugeVec,
    peer_user_transaction_disabled: IntGaugeVec,
    peer_last_update_generation: IntGaugeVec,
}

impl TransactionDenyConfigMetrics {
    pub fn new(registry: &Registry) -> Self {
        let active_peer_recommendations = IntGauge::new(
            "tx_deny_active_peer_recommendations",
            "Number of allowlisted peers with an accepted (Some) recommendation",
        )
        .unwrap();
        registry
            .register(Box::new(active_peer_recommendations.clone()))
            .unwrap();
        Self {
            // The `tx_deny_config_*` prefix matches the legacy gauges that lived in
            // `sui_config::node_config_metrics`, so existing dashboards keep working.
            local: DenyRulesGauges::new(registry, "tx_deny_config", "local config"),
            effective: DenyRulesGauges::new(
                registry,
                "tx_deny_effective",
                "effective config (local + peers)",
            ),
            active_peer_recommendations,
            peer_num_denied_objects: register_int_gauge_vec_with_registry!(
                "tx_deny_peer_num_denied_objects",
                "Number of objects in this peer's object_deny_list",
                &["peer"],
                registry,
            )
            .unwrap(),
            peer_num_denied_packages: register_int_gauge_vec_with_registry!(
                "tx_deny_peer_num_denied_packages",
                "Number of packages in this peer's package_deny_list",
                &["peer"],
                registry,
            )
            .unwrap(),
            peer_num_denied_addresses: register_int_gauge_vec_with_registry!(
                "tx_deny_peer_num_denied_addresses",
                "Number of addresses in this peer's address_deny_list",
                &["peer"],
                registry,
            )
            .unwrap(),
            peer_user_transaction_disabled: register_int_gauge_vec_with_registry!(
                "tx_deny_peer_user_transaction_disabled",
                "1 if this peer's recommendation sets user_transaction_disabled",
                &["peer"],
                registry,
            )
            .unwrap(),
            peer_last_update_generation: register_int_gauge_vec_with_registry!(
                "tx_deny_peer_last_update_generation",
                "Generation of the most recent accepted recommendation from this peer",
                &["peer"],
                registry,
            )
            .unwrap(),
        }
    }

    pub fn record(
        &self,
        local: &TransactionDenyConfig,
        peer_configs: &BTreeMap<AuthorityName, SharedTransactionDenyConfig>,
        effective: &TransactionDenyConfig,
    ) {
        self.local.set_from(local);
        self.effective.set_from(effective);

        let active = peer_configs
            .values()
            .filter(|m| m.rules().is_some())
            .count();
        self.active_peer_recommendations.set(active as i64);

        // Stale per-peer labels for peers that have left the allowlist hold their
        // last-known values until process restart; the allowlist is small and frozen
        // in production, so this is acceptable.
        for (authority, msg) in peer_configs {
            let label = format!("{}", authority.concise());
            let labels = &[label.as_str()];
            self.peer_last_update_generation
                .with_label_values(labels)
                .set(msg.generation() as i64);
            let (objects, packages, addresses, user_disabled) =
                msg.rules().map_or((0, 0, 0, 0), |r| {
                    (
                        r.object_deny_list.len() as i64,
                        r.package_deny_list.len() as i64,
                        r.address_deny_list.len() as i64,
                        r.user_transaction_disabled as i64,
                    )
                });
            self.peer_num_denied_objects
                .with_label_values(labels)
                .set(objects);
            self.peer_num_denied_packages
                .with_label_values(labels)
                .set(packages);
            self.peer_num_denied_addresses
                .with_label_values(labels)
                .set(addresses);
            self.peer_user_transaction_disabled
                .with_label_values(labels)
                .set(user_disabled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastcrypto::traits::VerifyingKey;
    use sui_config::transaction_deny_config::TransactionDenyConfigBuilder;
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::messages_consensus::SharedTransactionDenyConfigV1;
    use sui_types::transaction_deny_rules::TransactionDenyRules;

    fn name(byte: u8) -> AuthorityName {
        AuthorityName::new([byte; sui_types::crypto::AuthorityPublicKey::LENGTH])
    }

    fn obj(byte: u8) -> ObjectID {
        ObjectID::new([byte; 32])
    }

    fn addr(byte: u8) -> SuiAddress {
        SuiAddress::from_bytes([byte; 32]).unwrap()
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

    fn open_perpetual() -> (tempfile::TempDir, Arc<AuthorityPerpetualTables>) {
        let dir = tempfile::tempdir().unwrap();
        let perpetual = Arc::new(AuthorityPerpetualTables::open(dir.path(), None, None));
        (dir, perpetual)
    }

    /// `name(0)` is reserved as `self_authority` for the test manager. Tests should
    /// use `name(1)..` for peers.
    fn manager_with(
        local: TransactionDenyConfig,
        allowlist: PeerDenySyncConfig,
    ) -> (Arc<TransactionDenyConfigManager>, tempfile::TempDir) {
        let (dir, perpetual) = open_perpetual();
        let registry = Registry::new();
        let manager =
            TransactionDenyConfigManager::new(name(0), local, allowlist, perpetual, &registry)
                .unwrap();
        (manager, dir)
    }

    fn allowlist_for(authorities: &[AuthorityName]) -> PeerDenySyncConfig {
        PeerDenySyncConfig {
            peer_allowlist: authorities.iter().copied().collect(),
            broadcast_on_startup: false,
            broadcast_on_epoch_change: false,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn apply_update_rejects_non_allowlisted() {
        let local = TransactionDenyConfigBuilder::new().build();
        let allowed = name(1);
        let other = name(2);
        let (manager, _dir) = manager_with(local, allowlist_for(&[allowed]));

        let mut rules = TransactionDenyRules::default();
        rules.object_deny_list.insert(obj(7));
        manager
            .apply_update(make_msg(other, 1, Some(rules)))
            .unwrap();

        // Untrusted peer's rules must not affect the effective config.
        assert!(
            manager
                .effective_config()
                .load()
                .get_object_deny_set()
                .is_empty()
        );
        assert!(manager.peer_configs_snapshot().is_empty());
    }

    /// A self-broadcast loopback (msg.authority == self_authority) must be dropped
    /// even when self is on the allowlist — local_config is the source of truth for
    /// our own rules, and accepting a self-entry would let a stale persisted
    /// broadcast override later local-config edits across restarts.
    #[tokio::test(flavor = "multi_thread")]
    async fn apply_update_drops_self_loopback() {
        let local = TransactionDenyConfigBuilder::new().build();
        // Allowlist intentionally includes self (name(0)) to verify the self-check
        // wins over the allowlist check.
        let (manager, _dir) = manager_with(local, allowlist_for(&[name(0), name(1)]));

        let mut self_rules = TransactionDenyRules::default();
        self_rules.object_deny_list.insert(obj(42));
        manager
            .apply_update(make_msg(name(0), 1, Some(self_rules)))
            .unwrap();

        // Self entry must not be cached, persisted, or merged.
        assert!(
            manager.peer_configs_snapshot().is_empty(),
            "self-broadcast leaked into peer_configs",
        );
        assert!(
            manager
                .effective_config()
                .load()
                .get_object_deny_set()
                .is_empty(),
            "self-broadcast leaked into effective config",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn apply_update_ignores_stale_generations() {
        let local = TransactionDenyConfigBuilder::new().build();
        let peer = name(1);
        let (manager, _dir) = manager_with(local, allowlist_for(&[peer]));

        let mut newer = TransactionDenyRules::default();
        newer.object_deny_list.insert(obj(1));
        manager
            .apply_update(make_msg(peer, 100, Some(newer)))
            .unwrap();

        let mut older = TransactionDenyRules::default();
        older.object_deny_list.insert(obj(99)); // would be visible if accepted
        manager
            .apply_update(make_msg(peer, 50, Some(older)))
            .unwrap();

        let effective = manager.effective_config().load();
        assert!(effective.get_object_deny_set().contains(&obj(1)));
        assert!(!effective.get_object_deny_set().contains(&obj(99)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn apply_update_unions_with_local_and_other_peers() {
        let local = TransactionDenyConfigBuilder::new()
            .add_denied_address(addr(0))
            .build();
        let p1 = name(1);
        let p2 = name(2);
        let (manager, _dir) = manager_with(local, allowlist_for(&[p1, p2]));

        let mut r1 = TransactionDenyRules::default();
        r1.object_deny_list.insert(obj(1));
        r1.user_transaction_disabled = true;
        manager.apply_update(make_msg(p1, 1, Some(r1))).unwrap();

        let mut r2 = TransactionDenyRules::default();
        r2.address_deny_list.insert(addr(2));
        manager.apply_update(make_msg(p2, 1, Some(r2))).unwrap();

        let effective = manager.effective_config().load();
        assert!(effective.get_object_deny_set().contains(&obj(1)));
        assert!(effective.get_address_deny_set().contains(&addr(0)));
        assert!(effective.get_address_deny_set().contains(&addr(2)));
        assert!(effective.user_transaction_disabled());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn withdrawal_clears_peer_contribution_but_keeps_row() {
        let local = TransactionDenyConfigBuilder::new().build();
        let peer = name(1);
        let (manager, _dir) = manager_with(local, allowlist_for(&[peer]));

        let mut rules = TransactionDenyRules::default();
        rules.object_deny_list.insert(obj(1));
        manager
            .apply_update(make_msg(peer, 10, Some(rules)))
            .unwrap();
        assert!(
            manager
                .effective_config()
                .load()
                .get_object_deny_set()
                .contains(&obj(1))
        );

        manager.apply_update(make_msg(peer, 11, None)).unwrap();
        assert!(
            manager
                .effective_config()
                .load()
                .get_object_deny_set()
                .is_empty()
        );

        // Row is still present (with rules=None) so a delayed re-arrival of the
        // earlier generation cannot resurrect the recommendation.
        let snapshot = manager.peer_configs_snapshot();
        let entry = snapshot.get(&peer).unwrap();
        assert_eq!(entry.generation(), 11);
        assert!(entry.rules().is_none());

        // Verify the older `Some` is rejected.
        let mut stale = TransactionDenyRules::default();
        stale.object_deny_list.insert(obj(99));
        manager
            .apply_update(make_msg(peer, 10, Some(stale)))
            .unwrap();
        assert!(
            manager
                .effective_config()
                .load()
                .get_object_deny_set()
                .is_empty()
        );
    }

    // Note: cross-restart seeding-from-perpetual and committee-based pruning are
    // exercised end-to-end via sui-e2e-tests rather than as unit tests here, since
    // both depend on validator-style key material (committee construction validates
    // BLS pubkeys; perpetual seeding requires a fully-opened RocksDB).

    #[tokio::test(flavor = "multi_thread")]
    async fn allocate_next_broadcast_generation_is_monotonic() {
        let local = TransactionDenyConfigBuilder::new().build();
        let (manager, _dir) = manager_with(local, allowlist_for(&[]));

        let g1 = manager.allocate_next_broadcast_generation().unwrap();
        let g2 = manager.allocate_next_broadcast_generation().unwrap();
        let g3 = manager.allocate_next_broadcast_generation().unwrap();
        assert!(g2 > g1);
        assert!(g3 > g2);
    }
}
