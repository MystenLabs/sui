// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {
    use mysten_common::register_debug_fatal_handler;
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use sui_config::node::{ForkCrashBehavior, ForkRecoveryConfig};
    use sui_core::authority::{CheckpointTimeoutConfig, init_checkpoint_timeout_config};
    use sui_macros::{clear_fail_point, register_fail_point_arg, register_fail_point_if, sim_test};
    use sui_test_transaction_builder::make_transfer_sui_transaction;
    use sui_types::base_types::AuthorityName;
    use test_cluster::{TestCluster, TestClusterBuilder};

    async fn build_test_cluster() -> Arc<TestCluster> {
        TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(10_000)
            .with_num_unpruned_validators(4)
            .disable_fullnode_pruning()
            .build()
            .await
            .into()
    }

    fn node_to_authority_map(
        test_cluster: &TestCluster,
    ) -> HashMap<sui_simulator::task::NodeId, AuthorityName> {
        test_cluster
            .swarm
            .validator_nodes()
            .filter_map(|validator| {
                validator.get_node_handle().map(|handle| {
                    let node_id = handle.with(|node| node.get_sim_node_id());
                    (node_id, validator.name())
                })
            })
            .collect()
    }

    // Intercept fork fatal!s for forked validators, shutting the node down (so it can restart into
    // recovery) instead of aborting the sim.
    fn register_fork_kill_failpoints(
        forked_validators: Arc<Mutex<HashSet<AuthorityName>>>,
        checkpoint_overrides: Arc<Mutex<BTreeMap<u64, String>>>,
        resolve_authority: impl Fn(sui_simulator::task::NodeId) -> Option<AuthorityName>
        + Clone
        + Send
        + Sync
        + 'static,
    ) {
        register_fail_point_arg("kill_checkpoint_fork_node", {
            let forked_validators = forked_validators.clone();
            let checkpoint_overrides = checkpoint_overrides.clone();
            let resolve_authority = resolve_authority.clone();
            move || {
                let current = resolve_authority(sui_simulator::current_simnode_id())?;
                if forked_validators.lock().unwrap().contains(&current) {
                    Some(checkpoint_overrides.clone())
                } else {
                    None
                }
            }
        });
        register_fail_point_if("kill_transaction_fork_node", {
            let forked_validators = forked_validators.clone();
            let resolve_authority = resolve_authority.clone();
            move || {
                resolve_authority(sui_simulator::current_simnode_id())
                    .is_some_and(|current| forked_validators.lock().unwrap().contains(&current))
            }
        });
        // Split-brain bookkeeping: record the canonical (non-forked) digest into checkpoint_overrides.
        register_fail_point_arg("kill_split_brain_node", {
            let checkpoint_overrides = checkpoint_overrides.clone();
            let forked_validators = forked_validators.clone();
            move || Some((checkpoint_overrides.clone(), forked_validators.clone()))
        });
        // check_for_split_brain's debug_fatal! logs-and-continues in release; make the sim match that
        // (instead of panicking) so validators ride through split brain.
        register_debug_fatal_handler!(
            "Split brain detected in checkpoint signature aggregation",
            || {}
        );
    }

    fn clear_fork_kill_failpoints() {
        clear_fail_point("kill_checkpoint_fork_node");
        clear_fail_point("kill_transaction_fork_node");
        clear_fail_point("kill_split_brain_node");
    }

    // Executes one finalized transfer. The fork injection only touches non-system transactions, so
    // this is the sole traffic the tests need to trip a fork on demand.
    async fn execute_transfer(test_cluster: &TestCluster) {
        let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
        test_cluster.execute_transaction(tx).await;
    }

    async fn wait_until(timeout: Duration, description: &str, done: impl Fn() -> bool) {
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        panic!("condition not reached within {timeout:?}: {description}");
    }

    // Restarts `target` under RecoverOncePerVersion with a new binary version reported via the
    // override_binary_version failpoint (recovery refuses to clear a fork under the version that
    // produced it), then waits for the fork markers to clear.
    async fn recover_target_once_per_version(test_cluster: &TestCluster, target: AuthorityName) {
        register_fail_point_arg("override_binary_version", || {
            Some(Arc::new(Mutex::new("corrected-binary".to_string())))
        });

        let target_node = test_cluster
            .swarm
            .validator_nodes()
            .find(|validator| validator.name() == target)
            .unwrap();
        target_node.stop();
        tokio::time::sleep(Duration::from_secs(2)).await;
        {
            let mut cfg = target_node.config();
            cfg.fork_recovery = Some(ForkRecoveryConfig {
                transaction_overrides: Default::default(),
                checkpoint_overrides: Default::default(),
                fork_crash_behavior: ForkCrashBehavior::RecoverOncePerVersion,
            });
        }
        target_node.start().await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;

        clear_fork_kill_failpoints();
        clear_fail_point("override_binary_version");

        // The recovered validator cleared its fork markers.
        wait_until(
            Duration::from_secs(30),
            "fork markers should be cleared after auto-recovery",
            || {
                test_cluster
                    .swarm
                    .validator_nodes()
                    .find(|validator| validator.name() == target)
                    .unwrap()
                    .get_node_handle()
                    .is_some_and(|handle| {
                        handle.with(|node| {
                            let state = node.state();
                            let cp = state.get_checkpoint_store();
                            cp.get_checkpoint_fork_detected().unwrap().is_none()
                                && cp.get_transaction_fork_detected().unwrap().is_none()
                        })
                    })
            },
        )
        .await;
    }

    // A checkpoint fork (vs the transaction fork below): one validator participates in consensus
    // live with fork injection on the consensus/builder path, so its builder constructs a divergent
    // local checkpoint while the other three keep a quorum and certify the canonical one. After
    // the injection is cleared it restarts under RecoverOncePerVersion as a corrected binary,
    // clears its fork state, and rejoins.
    #[sim_test]
    async fn test_auto_fork_recovery_checkpoint_fork() {
        let test_cluster = build_test_cluster().await;

        let effects_overrides: Arc<Mutex<BTreeMap<String, String>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let checkpoint_overrides: Arc<Mutex<BTreeMap<u64, String>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let forked_validators: Arc<Mutex<HashSet<AuthorityName>>> =
            Arc::new(Mutex::new(HashSet::new()));

        let node_to_authority_map = node_to_authority_map(&test_cluster);

        // Fork exactly one validator so the other three keep a quorum.
        let target = test_cluster.swarm.validator_nodes().next().unwrap().name();

        // Baseline traffic before the injection is armed: these finalize with all four validators
        // agreeing.
        for _ in 0..2 {
            execute_transfer(&test_cluster).await;
        }

        register_fail_point_arg("simulate_fork_during_execution", {
            let forked_validators = forked_validators.clone();
            let effects_overrides = effects_overrides.clone();
            let node_to_authority_map = node_to_authority_map.clone();
            move || {
                let current = node_to_authority_map.get(&sui_simulator::current_simnode_id())?;
                if *current == target {
                    Some((
                        forked_validators.clone(),
                        /* full_halt: */ false,
                        effects_overrides.clone(),
                        /* fork_on_executor_path: */ false,
                    ))
                } else {
                    None
                }
            }
        });
        register_fork_kill_failpoints(forked_validators.clone(), checkpoint_overrides.clone(), {
            let node_to_authority_map = node_to_authority_map.clone();
            move |id| node_to_authority_map.get(&id).copied()
        });

        // Fork on demand: the injection is armed on the consensus/builder path only, so the first
        // transfer the target executes there diverges, its builder constructs a divergent
        // checkpoint, and detection fires when the canonical certified checkpoint reaches it
        // (kill_checkpoint_fork_node records the canonical digest). A transfer the target happens
        // to receive via the checkpoint executor instead — it was momentarily behind — is left
        // untouched by the injection, so drive further transfers until one lands on the builder
        // path.
        let detected = {
            let checkpoint_overrides = checkpoint_overrides.clone();
            move || !checkpoint_overrides.lock().unwrap().is_empty()
        };
        for _ in 0..10 {
            if detected() {
                break;
            }
            execute_transfer(&test_cluster).await;
            for _ in 0..12 {
                if detected() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        assert!(detected(), "target should detect a checkpoint fork");

        // Exactly one validator forked and detected a checkpoint fork.
        assert_eq!(forked_validators.lock().unwrap().len(), 1);
        assert_eq!(
            forked_validators.lock().unwrap().iter().next(),
            Some(&target)
        );

        // Corrected binary: stop injecting forks before recovering.
        clear_fail_point("simulate_fork_during_execution");

        recover_target_once_per_version(&test_cluster, target).await;

        // Liveness: every node, including the recovered validator, advances an epoch, proving it
        // rejoined (a fullnode-only wait could be satisfied by the other three).
        test_cluster.wait_for_next_epoch_all_nodes().await;
    }

    // A quorum-breaking fork: no checkpoint digest reaches quorum, so nothing certifies (split
    // brain). check_for_split_brain only logs in release (debug_fatal!), so recovery is manual:
    // restart with checkpoint_overrides naming the canonical digest. Reconvergence works because the
    // forked effects were never durably committed (the writeback dirty set is flushed only by the
    // checkpoint executor, on certified checkpoints), so the restart discards them and re-execution
    // is canonical; checkpoint_overrides clears the only durable forked state, locally_computed.
    #[sim_test]
    async fn test_split_brain_recovery_via_checkpoint_overrides() {
        // Split brain halts checkpoint certification until recovery, so the checkpoint executor
        // must not panic on the stall.
        init_checkpoint_timeout_config(CheckpointTimeoutConfig {
            warning_timeout: Duration::from_secs(5),
            panic_timeout: None,
        });

        let test_cluster = build_test_cluster().await;

        let effects_overrides: Arc<Mutex<BTreeMap<String, String>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let checkpoint_overrides: Arc<Mutex<BTreeMap<u64, String>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let forked_validators: Arc<Mutex<HashSet<AuthorityName>>> =
            Arc::new(Mutex::new(HashSet::new()));

        let node_to_authority_map = node_to_authority_map(&test_cluster);

        // full_halt forks validity-threshold stake (two of four validators), and the shared
        // effects_overrides map makes them fork identically, so no checkpoint digest can reach
        // quorum.
        register_fail_point_arg("simulate_fork_during_execution", {
            let forked_validators = forked_validators.clone();
            let effects_overrides = effects_overrides.clone();
            move || {
                Some((
                    forked_validators.clone(),
                    /* full_halt: */ true,
                    effects_overrides.clone(),
                    /* fork_on_executor_path: */ false,
                ))
            }
        });
        register_fork_kill_failpoints(forked_validators.clone(), checkpoint_overrides.clone(), {
            let node_to_authority_map = node_to_authority_map.clone();
            move |id| node_to_authority_map.get(&id).copied()
        });

        // Fork on demand: one user transaction is enough — it executes on every validator
        // post-consensus and the injection forks it on the two forked validators. It can never
        // finalize (quorum is unreachable), so submit without waiting for finality.
        let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
        let submit_task = tokio::spawn({
            let test_cluster = test_cluster.clone();
            async move {
                let _ = test_cluster.wallet.execute_transaction_may_fail(tx).await;
            }
        });

        // Wait until split brain is detected (kill_split_brain_node recorded the canonical digest).
        wait_until(
            Duration::from_secs(60),
            "expected a quorum-breaking fork with split brain detected",
            {
                let forked_validators = forked_validators.clone();
                let checkpoint_overrides = checkpoint_overrides.clone();
                move || {
                    forked_validators.lock().unwrap().len() > 1
                        && !checkpoint_overrides.lock().unwrap().is_empty()
                }
            },
        )
        .await;
        submit_task.abort();

        // Corrected binary: stop injecting forks and clear the kill failpoints so they can't
        // interrupt reconvergence.
        clear_fail_point("simulate_fork_during_execution");
        clear_fork_kill_failpoints();

        // Operator recovery: stop all (discards the in-memory forked effects), then restart with
        // checkpoint_overrides naming the canonical digest. Forked validators clear locally_computed
        // and re-execute canonically, so quorum re-forms.
        let overrides = checkpoint_overrides.lock().unwrap().clone();
        for validator in test_cluster.swarm.validator_nodes() {
            validator.stop();
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        for validator in test_cluster.swarm.validator_nodes() {
            {
                let mut cfg = validator.config();
                cfg.fork_recovery = Some(ForkRecoveryConfig {
                    transaction_overrides: Default::default(),
                    checkpoint_overrides: overrides.clone(),
                    fork_crash_behavior: ForkCrashBehavior::AwaitForkRecovery,
                });
            }
            validator
                .start()
                .await
                .expect("validator should restart under operator checkpoint_overrides recovery");
        }

        // Liveness: every node, including both recovered validators, advances an epoch — i.e. quorum
        // re-formed and the forked validators rejoined.
        test_cluster.wait_for_next_epoch_all_nodes().await;
    }

    // A transaction fork (vs the checkpoint fork above): a fallen-behind validator
    // re-executes a certified checkpoint's transactions via the checkpoint executor (where
    // expected_effects_digest is set) and diverges, tripping the per-transaction fork check. The
    // executor_path_only injection flag forks only on that path, guaranteeing a transaction fork; it
    // then recovers under RecoverOncePerVersion.
    #[sim_test]
    async fn test_auto_fork_recovery_transaction_fork() {
        let test_cluster = build_test_cluster().await;

        let effects_overrides: Arc<Mutex<BTreeMap<String, String>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let checkpoint_overrides: Arc<Mutex<BTreeMap<u64, String>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let forked_validators: Arc<Mutex<HashSet<AuthorityName>>> =
            Arc::new(Mutex::new(HashSet::new()));

        let target = test_cluster.swarm.validator_nodes().next().unwrap().name();

        // Stop the target so it falls behind the tip.
        test_cluster
            .swarm
            .validator_nodes()
            .find(|validator| validator.name() == target)
            .unwrap()
            .stop();

        // The fork injection skips system transactions, so the target's catch-up must
        // re-execute a user transaction it has never executed. Land one to checkpoint
        // finality: submitted after the stop, the target cannot have executed it. Retrying
        // the same signed transaction on transient failures cannot equivocate the gas object.
        let tx_data = test_cluster
            .test_transaction_builder()
            .await
            .transfer_sui(Some(1), test_cluster.get_address_1())
            .build();
        let tx = test_cluster.sign_transaction(&tx_data).await;
        while test_cluster
            .wallet
            .execute_transaction_may_fail(tx.clone())
            .await
            .is_err()
        {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Wait for the epoch holding the transfer to close: a closed epoch can only be
        // replayed through the checkpoint executor (peers tear down its consensus at
        // reconfig), so the target's catch-up must re-execute the transfer there — and fork.
        let finality_epoch = test_cluster
            .fullnode_handle
            .sui_node
            .with(|node| node.state().epoch_store_for_testing().epoch());
        test_cluster.wait_for_epoch(Some(finality_epoch + 1)).await;

        // Arm the injection and kill hooks BEFORE restarting the target: its catch-up
        // re-execution can complete inside start()'s internal awaits, so arming afterwards
        // races the executor and loses on some schedules (the fork window closes forever once
        // catch-up finishes — and a fork tripped before the kill hooks are armed would
        // fatal!-abort the sim). The target's new sim node id is unknowable until start()
        // returns, so match by exclusion instead: while the target is down, snapshot the sim
        // ids of every running node — any id outside that set executing transactions must be
        // the restarted target (nothing else starts during this test).
        let known_other_ids: HashSet<sui_simulator::task::NodeId> = test_cluster
            .swarm
            .all_nodes()
            .filter_map(|node| {
                node.get_node_handle()
                    .map(|handle| handle.with(|n| n.get_sim_node_id()))
            })
            .collect();
        let node_to_authority_map = node_to_authority_map(&test_cluster);

        // Fork the target only on the executor path, so its catch-up re-execution trips the tx check.
        register_fail_point_arg("simulate_fork_during_execution", {
            let forked_validators = forked_validators.clone();
            let effects_overrides = effects_overrides.clone();
            let known_other_ids = known_other_ids.clone();
            move || {
                if known_other_ids.contains(&sui_simulator::current_simnode_id()) {
                    None
                } else {
                    Some((
                        forked_validators.clone(),
                        /* full_halt: */ false,
                        effects_overrides.clone(),
                        /* fork_on_executor_path: */ true,
                    ))
                }
            }
        });
        register_fork_kill_failpoints(forked_validators.clone(), checkpoint_overrides.clone(), {
            let node_to_authority_map = node_to_authority_map.clone();
            let known_other_ids = known_other_ids.clone();
            move |id| {
                node_to_authority_map
                    .get(&id)
                    .copied()
                    .or_else(|| (!known_other_ids.contains(&id)).then_some(target))
            }
        });

        test_cluster
            .swarm
            .validator_nodes()
            .find(|validator| validator.name() == target)
            .unwrap()
            .start()
            .await
            .unwrap();

        // Wait until the target forks (after which kill_transaction_fork_node shuts it down).
        wait_until(
            Duration::from_secs(60),
            "target should transaction-fork while catching up on the executor path",
            {
                let forked_validators = forked_validators.clone();
                move || forked_validators.lock().unwrap().contains(&target)
            },
        )
        .await;

        // Corrected binary: stop injecting forks before recovering.
        clear_fail_point("simulate_fork_during_execution");

        // Confirm it was a transaction fork: checkpoint_overrides (set only by the checkpoint/split-
        // brain failpoints) is empty even though the target forked.
        assert!(
            checkpoint_overrides.lock().unwrap().is_empty(),
            "expected a transaction fork (no checkpoint fork should have been recorded)"
        );

        recover_target_once_per_version(&test_cluster, target).await;

        // Liveness: every node, including the recovered validator, advances an epoch.
        test_cluster.wait_for_next_epoch_all_nodes().await;
    }
}
