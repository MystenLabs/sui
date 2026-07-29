// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod test {
    use mysten_common::register_debug_fatal_handler;
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use sui_config::node::{ForkCrashBehavior, ForkRecoveryConfig};
    use sui_core::authority::{
        CheckpointTimeoutConfig, SimulatedForkConfig, init_checkpoint_timeout_config,
    };
    use sui_core::checkpoints::CheckpointStore;
    use sui_macros::{clear_fail_point, register_fail_point_arg, register_fail_point_if, sim_test};
    use sui_simulator::task::NodeId;
    use sui_test_transaction_builder::make_transfer_sui_transaction;
    use sui_types::base_types::AuthorityName;
    use test_cluster::{TestCluster, TestClusterBuilder};
    use tracing::info;

    async fn build_test_cluster() -> Arc<TestCluster> {
        sui_protocol_config::ProtocolConfig::poison_get_for_min_version();
        TestClusterBuilder::new()
            .with_num_validators(4)
            .with_epoch_duration_ms(10_000)
            .with_num_unpruned_validators(4)
            .disable_fullnode_pruning()
            .build()
            .await
            .into()
    }

    fn authorities_by_node(test_cluster: &TestCluster) -> HashMap<NodeId, AuthorityName> {
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

    // Runs `f` against the checkpoint store of `name`'s node, or returns None if it is down.
    fn with_checkpoint_store<T>(
        test_cluster: &TestCluster,
        name: AuthorityName,
        f: impl FnOnce(&CheckpointStore) -> T,
    ) -> Option<T> {
        test_cluster
            .swarm
            .validator_nodes()
            .find(|validator| validator.name() == name)?
            .get_node_handle()
            .map(|handle| handle.with(|node| f(node.state().get_checkpoint_store())))
    }

    // Bookkeeping shared with the fork failpoints. `forked_validators` is written by the injection
    // as soon as it diverges a validator; `transaction_forks` and `checkpoint_overrides` are
    // written by the kill hooks, which run only after the node has recorded an actual fork. Wait
    // on the latter two when you need proof that detection fired, not just that the injection did.
    #[derive(Clone, Default)]
    struct ForkHarness {
        forked_validators: Arc<Mutex<HashSet<AuthorityName>>>,
        transaction_forks: Arc<Mutex<HashSet<AuthorityName>>>,
        checkpoint_overrides: Arc<Mutex<BTreeMap<u64, String>>>,
    }

    impl ForkHarness {
        // Arms the fork injection on every validator `should_fork` accepts, plus the kill hooks
        // that intercept the fork fatal!s and shut the node down (so it can restart into recovery)
        // instead of aborting the sim.
        //
        // `resolve_authority` maps a sim node id to its authority; it is a closure rather than a
        // map because a validator restarted mid-test gets a new, unpredictable sim node id.
        fn arm(
            &self,
            split_brain: bool,
            fork_on_executor_path: bool,
            resolve_authority: impl Fn(NodeId) -> Option<AuthorityName> + Clone + Send + Sync + 'static,
            should_fork: impl Fn(AuthorityName) -> bool + Clone + Send + Sync + 'static,
        ) {
            register_fail_point_arg("simulate_fork_during_execution", {
                let forked_validators = self.forked_validators.clone();
                let resolve_authority = resolve_authority.clone();
                let should_fork = should_fork.clone();
                move || {
                    let current = resolve_authority(sui_simulator::current_simnode_id())?;
                    should_fork(current).then(|| SimulatedForkConfig {
                        forked_validators: forked_validators.clone(),
                        split_brain,
                        fork_on_executor_path,
                    })
                }
            });

            register_fail_point_arg("kill_checkpoint_fork_node", {
                let forked_validators = self.forked_validators.clone();
                let checkpoint_overrides = self.checkpoint_overrides.clone();
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
            // Reached only after record_transaction_fork_detected, so recording here is proof that
            // the effects-digest check tripped, not merely that the injection fired.
            register_fail_point_if("kill_transaction_fork_node", {
                let forked_validators = self.forked_validators.clone();
                let transaction_forks = self.transaction_forks.clone();
                let resolve_authority = resolve_authority.clone();
                move || {
                    let Some(current) = resolve_authority(sui_simulator::current_simnode_id())
                    else {
                        return false;
                    };
                    if !forked_validators.lock().unwrap().contains(&current) {
                        return false;
                    }
                    transaction_forks.lock().unwrap().insert(current);
                    true
                }
            });
            // Split-brain bookkeeping: record the canonical (non-forked) digest.
            register_fail_point_arg("kill_split_brain_node", {
                let checkpoint_overrides = self.checkpoint_overrides.clone();
                let forked_validators = self.forked_validators.clone();
                move || Some((checkpoint_overrides.clone(), forked_validators.clone()))
            });
            // check_for_split_brain's debug_fatal! logs-and-continues in release; make the sim
            // match that (instead of panicking) so validators ride through split brain.
            register_debug_fatal_handler!(
                "Split brain detected in checkpoint signature aggregation",
                || {}
            );
        }

        fn disarm_injection(&self) {
            clear_fail_point("simulate_fork_during_execution");
        }

        fn disarm_kill_hooks(&self) {
            clear_fail_point("kill_checkpoint_fork_node");
            clear_fail_point("kill_transaction_fork_node");
            clear_fail_point("kill_split_brain_node");
        }

        fn forked(&self) -> HashSet<AuthorityName> {
            self.forked_validators.lock().unwrap().clone()
        }

        fn transaction_forked(&self) -> HashSet<AuthorityName> {
            self.transaction_forks.lock().unwrap().clone()
        }

        // Every checkpoint fork / split brain detected, as the `seq -> canonical digest` map an
        // operator would pass to ForkRecoveryConfig::checkpoint_overrides.
        fn checkpoint_overrides(&self) -> BTreeMap<u64, String> {
            self.checkpoint_overrides.lock().unwrap().clone()
        }

        // The earliest sequence at which a checkpoint fork or split brain was detected, if any.
        fn forked_seq(&self) -> Option<u64> {
            self.checkpoint_overrides
                .lock()
                .unwrap()
                .keys()
                .next()
                .copied()
        }
    }

    // Executes one finalized transfer. The fork injection only touches non-system transactions, so
    // this is the sole traffic the tests need to trip a fork on demand.
    async fn execute_transfer(test_cluster: &TestCluster) {
        let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
        test_cluster.execute_transaction(tx).await;
    }

    async fn wait_for(timeout: Duration, done: impl Fn() -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            if done() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        false
    }

    async fn wait_until(timeout: Duration, description: &str, done: impl Fn() -> bool) {
        assert!(
            wait_for(timeout, done).await,
            "condition not reached within {timeout:?}: {description}"
        );
    }

    // Restarts `target` under RecoverOncePerVersion with a new binary version reported via the
    // override_binary_version failpoint (recovery refuses to clear a fork under the version that
    // produced it), disarms the kill hooks so they cannot interrupt reconvergence, then waits for
    // the fork markers to clear.
    async fn recover_forked_target(
        test_cluster: &TestCluster,
        harness: &ForkHarness,
        target: AuthorityName,
    ) {
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

        harness.disarm_kill_hooks();
        clear_fail_point("override_binary_version");

        wait_until(
            Duration::from_secs(30),
            "fork markers should be cleared after auto-recovery",
            || {
                with_checkpoint_store(test_cluster, target, |cp| {
                    cp.get_checkpoint_fork_detected().unwrap().is_none()
                        && cp.get_transaction_fork_detected().unwrap().is_none()
                })
                .unwrap_or(false)
            },
        )
        .await;
    }

    // Liveness (advancing an epoch) only proves a recovered validator rejoined; it would not catch
    // one that rejoined carrying divergent history. Checkpoint digests chain, so agreeing with the
    // fullnode on a checkpoint past `min_seq` — the sequence where the fork was injected — proves
    // the validator's whole history up to that point is canonical again.
    async fn assert_converged_with_fullnode(
        test_cluster: &TestCluster,
        name: AuthorityName,
        min_seq: u64,
    ) {
        wait_until(
            Duration::from_secs(60),
            &format!("{name:?} should agree with the fullnode past checkpoint {min_seq}"),
            || {
                let Some(Some(local_tip)) = with_checkpoint_store(test_cluster, name, |cp| {
                    cp.get_highest_executed_checkpoint().unwrap()
                }) else {
                    return false;
                };
                let seq = *local_tip.sequence_number();
                if seq <= min_seq {
                    return false;
                }
                test_cluster
                    .fullnode_handle
                    .sui_node
                    .with(|node| {
                        node.state()
                            .get_checkpoint_store()
                            .get_checkpoint_by_sequence_number(seq)
                            .unwrap()
                    })
                    .is_some_and(|canonical| canonical.digest() == local_tip.digest())
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
        let harness = ForkHarness::default();
        let authorities = authorities_by_node(&test_cluster);

        // Fork exactly one validator so the other three keep a quorum.
        let target = test_cluster.swarm.validator_nodes().next().unwrap().name();

        harness.arm(
            /* split_brain */ false,
            /* fork_on_executor_path */ false,
            move |id| authorities.get(&id).copied(),
            move |name| name == target,
        );

        // Fork on demand: the injection is armed on the consensus/builder path only, so the first
        // transfer the target executes there diverges, its builder constructs a divergent
        // checkpoint, and detection fires when the canonical certified checkpoint reaches it
        // (kill_checkpoint_fork_node records the canonical digest). A transfer the target happens
        // to receive via the checkpoint executor instead — it was momentarily behind — is left
        // untouched by the path-exclusive injection, so drive further transfers until one lands on
        // the builder path.
        let mut transfers = 0;
        while harness.forked_seq().is_none() && transfers < 10 {
            transfers += 1;
            execute_transfer(&test_cluster).await;
            wait_for(Duration::from_secs(6), || harness.forked_seq().is_some()).await;
        }
        let forked_seq = harness
            .forked_seq()
            .expect("target should detect a checkpoint fork");
        // Logged so the retry bound above stays justified by observation rather than assumption:
        // if this is always 1, the loop can collapse to a single transfer.
        info!(transfers, forked_seq, "checkpoint fork detected");

        assert_eq!(harness.forked(), HashSet::from([target]));

        // Corrected binary: stop injecting forks before recovering.
        harness.disarm_injection();
        recover_forked_target(&test_cluster, &harness, target).await;

        // Liveness: every node, including the recovered validator, advances an epoch, proving it
        // rejoined (a fullnode-only wait could be satisfied by the other three).
        test_cluster.wait_for_next_epoch_all_nodes().await;

        assert_converged_with_fullnode(&test_cluster, target, forked_seq).await;
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
        let harness = ForkHarness::default();
        let authorities = authorities_by_node(&test_cluster);

        // split_brain forks past the validity threshold (two of four validators), and every forked
        // validator diverges on the same transaction, so no checkpoint digest can reach quorum.
        harness.arm(
            /* split_brain */ true,
            /* fork_on_executor_path */ false,
            move |id| authorities.get(&id).copied(),
            |_| true,
        );

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
            || harness.forked().len() > 1 && harness.forked_seq().is_some(),
        )
        .await;
        submit_task.abort();

        let forked_seq = harness.forked_seq().unwrap();

        // Corrected binary: stop injecting forks and clear the kill failpoints so they can't
        // interrupt reconvergence.
        harness.disarm_injection();
        harness.disarm_kill_hooks();

        // Operator recovery: stop all (discards the in-memory forked effects), then restart with
        // checkpoint_overrides naming the canonical digest. Forked validators clear locally_computed
        // and re-execute canonically, so quorum re-forms.
        let overrides = harness.checkpoint_overrides();
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

        let names: Vec<_> = test_cluster
            .swarm
            .validator_nodes()
            .map(|validator| validator.name())
            .collect();
        for name in names {
            assert_converged_with_fullnode(&test_cluster, name, forked_seq).await;
        }
    }

    // A transaction fork (vs the checkpoint fork above): a fallen-behind validator
    // re-executes a certified checkpoint's transactions via the checkpoint executor (where
    // expected_effects_digest is set) and diverges, tripping the per-transaction fork check. The
    // fork_on_executor_path injection flag forks only on that path, guaranteeing a transaction
    // fork; it then recovers under RecoverOncePerVersion.
    #[sim_test]
    async fn test_auto_fork_recovery_transaction_fork() {
        let test_cluster = build_test_cluster().await;
        let harness = ForkHarness::default();

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
        let tx = make_transfer_sui_transaction(
            &test_cluster.wallet,
            Some(test_cluster.get_address_1()),
            Some(1),
        )
        .await;
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
        let known_other_ids: HashSet<NodeId> = test_cluster
            .swarm
            .all_nodes()
            .filter_map(|node| {
                node.get_node_handle()
                    .map(|handle| handle.with(|n| n.get_sim_node_id()))
            })
            .collect();
        let authorities = authorities_by_node(&test_cluster);

        // Fork the target only on the executor path, so its catch-up re-execution trips the tx check.
        harness.arm(
            /* split_brain */ false,
            /* fork_on_executor_path */ true,
            move |id| {
                authorities
                    .get(&id)
                    .copied()
                    .or_else(|| (!known_other_ids.contains(&id)).then_some(target))
            },
            move |name| name == target,
        );

        let fork_point = test_cluster.fullnode_handle.sui_node.with(|node| {
            node.state()
                .get_checkpoint_store()
                .get_highest_executed_checkpoint_seq_number()
                .unwrap()
                .unwrap_or(0)
        });

        test_cluster
            .swarm
            .validator_nodes()
            .find(|validator| validator.name() == target)
            .unwrap()
            .start()
            .await
            .unwrap();

        // Waits on transaction_forks, which the kill hook writes only after the effects-digest
        // check tripped and the fork was recorded. Waiting on forked() instead would be satisfied
        // by the injection merely firing, letting the rest of the test pass vacuously.
        wait_until(
            Duration::from_secs(60),
            "target should transaction-fork while catching up on the executor path",
            || harness.transaction_forked().contains(&target),
        )
        .await;

        // Corrected binary: stop injecting forks before recovering.
        harness.disarm_injection();

        // Confirm it was a transaction fork: the checkpoint/split-brain hooks recorded nothing
        // even though the target forked.
        assert!(
            harness.forked_seq().is_none(),
            "expected a transaction fork (no checkpoint fork should have been recorded)"
        );

        recover_forked_target(&test_cluster, &harness, target).await;

        // Liveness: every node, including the recovered validator, advances an epoch.
        test_cluster.wait_for_next_epoch_all_nodes().await;

        assert_converged_with_fullnode(&test_cluster, target, fork_point).await;
    }
}
