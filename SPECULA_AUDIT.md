# Specula audit of Sui: master plan

This file tracks the full Specula audit of Sui. Each component below is a candidate for its own
Specula run, with its own guidance file and worktree. The earlier run `sui-exec-1`
(worktree `~/projects/mlogan-sui-specula`, notes in `SPECULA_PROGRESS.md` there) covered the
commit-to-execution path and produced MC-1 (draft PR #28679).

## Major components

Priority reflects how much distributed or concurrent protocol logic a component has. Specula
finds the most bugs where interleavings, crashes, and message reordering matter.

| # | Component | Main code | Priority | Prior coverage |
|---|-----------|-----------|----------|----------------|
| 1 | Mysticeti consensus core | `consensus/core/src` (`core.rs`, `dag_state.rs`, `universal_committer.rs`, `base_committer.rs`, `flex_committer.rs`, `leader_schedule*.rs`, `linearizer.rs`, `commit_finalizer.rs`, `transaction_vote_tracker.rs`) | High | None |
| 2 | Consensus sync and recovery | `consensus/core/src` (`synchronizer.rs`, `commit_syncer.rs`, `block_manager.rs`, `subscriber.rs`, `round_prober.rs`, `storage/`, `observer_*.rs`) | High | None |
| 3 | Consensus submission | `sui-core/src` (`consensus_adapter.rs`, `consensus_transaction_pool.rs`, `admission_queue.rs`, `staggered_submission.rs`, `consensus_validator.rs`, `mysticeti_adapter.rs`, `consensus_manager/`) | Medium | None |
| 4 | Consensus commit handling | `consensus_handler.rs`, `authority/consensus_quarantine.rs`, `authority/shared_object_version_manager.rs`, `authority/transaction_deferral.rs`, congestion trackers, `post_consensus_tx_reorder.rs`, `consensus_commit_summary.rs` | High | Partial (`sui-exec-1`) |
| 5 | Execution scheduling | `execution_scheduler/` (`causal_admission.rs`, `settlement_scheduler.rs`, `funds_withdraw_scheduler/`), `execution_driver.rs` | High | Yes (`sui-exec-1`) |
| 6 | Accumulators and address balances | `accumulators/` (`balances.rs`, `coin_reservations.rs`, `object_funds_checker/`, `unsettled_object_withdrawals.rs`), settlement transactions | High | Partial (`sui-exec-1`) |
| 7 | Execution cache and object store | `execution_cache/writeback_cache.rs`, `execution_cache/object_locks.rs`, `authority/authority_store.rs`, `global_state_hasher.rs` | Medium | None |
| 8 | Checkpoint building and certification | `checkpoints/mod.rs` (builder, signature aggregation), `checkpoints/causal_order.rs`, `locally_built_checkpoints.rs` | High | None |
| 9 | Checkpoint execution | `checkpoints/checkpoint_executor/` | High | None |
| 10 | State sync | `sui-network/src/state_sync/`, `sui-network/src/discovery/` | High | None |
| 11 | Reconfiguration and epoch change | `epoch/reconfiguration.rs`, `authority/authority_per_epoch_store.rs` lifecycle, `authority/epoch_start_configuration.rs`, `epoch/committee_store.rs`, reconfigure loop in `sui-node/src/lib.rs`, end-of-publish and change-epoch tx | High | Epoch boundary only (`sui-exec-1`) |
| 12 | Randomness (DKG and beacon) | `epoch/randomness.rs`, `sui-network/src/randomness/`, `randomness_round_receiver.rs` | Medium | None |
| 13 | Protocol upgrade voting | capabilities handling in `authority.rs` and `consensus_handler.rs`, system package upgrade at epoch change | Medium | None |
| 14 | Transaction submission client | `transaction_driver/` (`effects_certifier.rs`, `transaction_submitter.rs`, `reconfig_observer.rs`), `transaction_orchestrator.rs`, `validator_client_monitor/` | Medium | None |
| 15 | Validator request path | `authority_server.rs`, `transaction_signing_filter.rs`, `overload_monitor.rs`, `traffic_controller/` | Low | None |
| 16 | Pruning | `authority/authority_store_pruner.rs`, `authority/authority_per_epoch_store_pruner.rs`, `epoch/consensus_store_pruner.rs` | Medium | None |
| 17 | JWK and authenticator state | JWK votes in `authority_per_epoch_store.rs` and `consensus_handler.rs` | Low | None |
| 18 | Move execution layer | `sui-execution/`, `external-crates/move` | Low | Not a good fit: the code is deterministic and single-threaded |
| 19 | Authorization and authentication | `sui-types/src` (`signature.rs`, `signature_verification.rs`, `multisig.rs`, `zk_login_authenticator.rs`, `passkey_authenticator.rs`, `address_alias.rs`), `sui-core/src/signature_verifier.rs`, alias handling in `authority_per_epoch_store.rs` and `epoch_start_configuration.rs`, transaction voting in `consensus_validator.rs` and `consensus/core/src/transaction_vote_tracker.rs` | High | None |
| 20 | Transaction validity invariants | `TransactionData::validity_check` in `sui-types/src/transaction.rs`, `sui-transaction-checks/`, `transaction_input_loader.rs`, `transaction_deny_config_manager.rs`, `gasless_rate_limiter.rs`, checks in `authority_server.rs` and `consensus_validator.rs` | High | None |

Component 20 asks whether any validity check that safety relies on can be bypassed. Examples are a check that runs at signing but not on the consensus or checkpoint path, a check that the protocol version or epoch changes between signing and execution, and a check that a system transaction or a rewritten transaction skips.

Crash recovery and restart cut across components 2, 4, 8, 9, 11, and 16. Each run for those
components should model crashes at every persistence point.

## Status

## Run order

1. `sui-auth-1`: components 19 and 20. Guidance: `specula/sui-auth-guidance.md`.
2. Components 8, 9, 10 (checkpoint building, checkpoint execution, state sync).
3. Components 11, 12, 13 (reconfiguration, randomness, protocol upgrade voting).
4. Components 1, 2 (consensus core and sync).
5. Remaining medium-priority components.

## Status

- [x] List major components
- [x] Choose run order
- [x] Write guidance file for `sui-auth-1`
- [ ] Start `sui-auth-1`
