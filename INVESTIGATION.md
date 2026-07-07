# Investigation: validator fork after SIGTERM→SIGKILL restart

## Symptom
- Occurs in production after operators upgrade (restart) a validator; never in automated tests.
- After restart, the node re-processes a consensus commit whose ConsensusCommitPrologue (CCP)
  was already executed via the checkpoint executor, but computes a *different* CCP.
- The divergent transaction writes an output object version that already exists, tripping the
  writeback cache assert that object versions increase monotonically.

## Lead
- SIGTERM → graceful shutdown begins → SIGKILL before it completes.
- This sequence is never exercised by automated tests (tests hard-kill only).
- Hypothesis: some state is persisted only during graceful shutdown (or with asymmetric
  durability, e.g. no-WAL writes durable only via memtable flush), so a partial graceful
  shutdown produces an inconsistent cut across stores that a pure hard kill never produces.

## Status
- [x] Fan out readers: shutdown path, RocksDB durability/WAL, consensus replay boundary,
      CCP construction determinism, checkpoint-executor/CCP race + git history.
- [x] Synthesize findings; identify candidate root cause(s).
- [x] Verify load-bearing code directly (main.rs signal path; checkpoint executor split
      commit; get_or_init_next_object_versions; DKG persistence via quarantine).

## SYNTHESIS (final)

### The three ingredients

**1. SIGTERM performs no graceful shutdown, and runtime-drop semantics create a specific torn
cut (verified).** `wait_termination` (main.rs:211-226) just returns on SIGTERM; the only
teardown is `drop(runtimes)` (main.rs:200). Dropping a tokio Runtime cancels async tasks at
their next await but RUNS IN-FLIGHT `spawn_blocking` CLOSURES TO COMPLETION. In
`execute_checkpoint` (checkpoint_executor/mod.rs:436-457):
  (A) perpetual-DB commit of checkpoint K's outputs (incl. CCP effects) runs in spawn_blocking
      → completes;
  (B) `handle_finalized_checkpoint` → quarantine flush to epoch DB (last_consensus_stats,
      next-version advances, debts, observations, processed-markers) is the async continuation
      → cancelled, never issued.
So EVERY SIGTERM that catches a checkpoint mid-commit durably persists executed CCP effects
while leaving the consensus resume pointer P behind. A hard kill can only produce this cut in
the microsecond window between the two batch writes. SIGKILL-after-SIGTERM isn't even required.
Additionally the #25442 drain-point gating means P can trail the executed-checkpoint watermark
by more than one checkpoint on any crash.

**2. Replay of already-executed commits is unguarded.** On restart, consensus replays all
commits > P (P from epoch DB only; consensus_manager/mod.rs:302-318) and `handle_consensus_commit`
regenerates the CCP for each (consensus_handler.rs:3157-3170). Nothing consults the executed-
checkpoint watermark; nothing compares the regenerated CCP against the one already durably
executed (its digest is in the executed checkpoint). Safety rests ENTIRELY on the regenerated
CCP being bit-identical.

**3. The divergence source.** CCP digest inputs that aren't pure functions of the stored commit:
`consensus_determined_version_assignments` (congestion cancellations ← ExecutionTimeEstimator
estimates, debts, deferred txns, DKG state) and `additional_state_digest` (ring buffer d1 +
IndirectStateObserver d2 = per-tx estimator estimates). All the backing tables ARE flushed
atomically with P in one quarantine batch, and replay re-derives them deterministically —
WITH THE SAME BINARY. The failures occur "after operators upgrade their node": the canonical
CCP on disk was computed by the pre-upgrade binary (locally or via other validators through
state sync); the replay recomputes it with the POST-upgrade binary. Any non-protocol-gated
behavioral drift in estimator/congestion/digest logic forks the digest. Tests restart with the
same binary and never take the SIGTERM path, so both the torn cut and the cross-version replay
are unexercised.

### Failure sequence
SIGTERM during upgrade → A-done/B-skipped cut (CCP effects for commits (P, K] on disk, resume
pointer at P) → restart on new binary → consensus replays (P, K] → regenerated CCP differs
(estimator/congestion drift or any rebuilt-state mismatch) → different digest, not deduped →
executes → writes shared object (e.g. Clock 0x6) at a version already on disk →
CachedVersionMap::insert `fatal!` (cache_types.rs:50-62, fires on `>=`).

### Confirmations to seek from production data
- Diff the two CCP transactions (executed-in-checkpoint vs. replayed): which field differs —
  version_assignments vs additional_state_digest — discriminates congestion-cancellation drift
  from estimator-digest drift.
- Check incident nodes' last_consensus_stats vs highest_executed_checkpoint at crash.
- Check whether the upgrade in question changed execution_time_estimator / congestion tracker /
  commit-interval logic without a protocol-version gate.

## ROUND 2 — user constraints narrow the search
User input: (1) observed divergence came from differing execution time estimates; (2) Antithesis
tests cross-version upgrades → NOT binary drift; (3) `fail_point!("batch-write-before")`
already exercises restarts between the perpetual write (A) and epoch write (B); (4) design
assumes replay never diverges — find the broken determinism, don't guard.

Reframed: estimator state (or the set of txns whose estimates are hashed/costed) at replay is
not a pure function of {tables@P + replayed commits}. Verified replay-PURE so far (myself):
- SIGTERM path confirmed: main.rs:211-226 → drop(runtimes); A in spawn_blocking
  (checkpoint_executor/mod.rs:436-451) completes, B (:455-457) cancelled.
- process_execution_time_observations (consensus_handler.rs:2137-2168): applies AND stages
  unconditionally; table keyed (generation, authority) (consensus_quarantine.rs:398-403);
  rebuild (authority_per_epoch_store.rs:968-998) = max-generation-wins per (key, source),
  scan-order-independent (strict > check at execution_time_estimator.rs:813) — symmetric with
  live semantics. Note: insert_batch overwrite hazard if same (gen, authority) staged twice
  with different key sets — but share path increments generation per tx
  (execution_time_estimator.rs:599-601, seeded from SystemTime::now() micros at :213) and
  dedup drops exact dups, so honest nodes can't collide. (Byzantine could — side note.)
- filter_consensus_txns (consensus_handler.rs:2453-2781): reconfig state, end_of_publish,
  owned-object lock maps — all quarantine-consistent; lock check
  (authority_per_epoch_store.rs:1825-1861) is closed-world over the two lock maps, no live
  object store read; self-locks (same digest) pass, so replay re-acquisition is symmetric.
- get_or_init_next_object_versions (:2078-2161): direct multi_insert makes first-touch inits
  durable at processing time → replay reads the same table entry; init-from-live-store only on
  true first touch where the value is canonical. Mostly symmetric.
- DKG/randomness tables: quarantined (consensus_quarantine.rs:339-359).

Remaining unverified surfaces (agents dispatched):
A. Rejected-transaction sets on replay: finalized_commits store, CommitFinalizer flush-before-
   send atomicity, recovery recompute path (recovered_rejected_transactions=false), vote
   tracker rebuild, GC round. Different rejected set → different costed tx set → "differing
   estimates" in d2 → divergent CCP.
B. Congestion debt decay symmetry: debts stored as (round, debt) snapshots; decay math on load
   vs live evolution; deferred-txn table symmetry.
C. Line-by-line sweep of collect_transactions_to_schedule / deferral+cancellation /
   assign_versions_from_consensus / CCP assembly / IndirectStateObserver call sites for impure
   reads (live object store, executed-status caches, wall clock).

Result A (rejected-tx replay): STRUCTURAL ASYMMETRY FOUND.
- Any commit the finalizer SENT to the handler has its rejected set durably in
  `finalized_commits` (add_finalized_commit + flush BEFORE send, commit_finalizer.rs:131-141;
  single atomic WriteBatch). Replay reloads it verbatim (commit.rs:489-492,
  recovered_rejected_transactions=true) → already_finalized branch → byte-identical. SAFE.
- BUT commits routinely sit in the `commits` CF without `finalized_commits` rows: Linearizer
  adds commits synchronously (linearizer.rs:239) and proposer/core/authority_service flush them
  (proposer.rs:611, core.rs:1239), while the finalizer buffers ≥3 leader rounds before
  finalizing. On restart those get recovered_rejected_transactions=false → RE-FINALIZED via
  process_commit.
- Re-finalization vote-set asymmetry: live finalization aggregates reject votes from ALL
  received blocks (TransactionVoteTracker fed on block receipt, incl. uncommitted blocks);
  replay re-finalization feeds the tracker only committed sub-dag blocks
  (recover_and_vote_on_blocks, commit_observer.rs:232-233). recover_blocks_after_round (which
  would restore uncommitted-block votes from the blocks CF) runs AFTER recover_and_send_commits
  and RACES the finalizer task (commit_observer.rs:71-85) — labelled "for future commits".
- Consequence: a tx whose live path was reject-vote→pending→indirect-resolve can be directly
  accepted on replay (or the race makes it nondeterministic). Different rejected set → different
  accepted set fed to costing → different d2 estimates hash → divergent CCP.
- CAVEAT (mine): commit-synced nodes re-finalize from committed-blocks-only routinely without
  forking, so the protocol may guarantee vote-set differences can't flip final outcomes; the
  suspicious part is the pending-vs-direct path difference + the recovery race. Needs
  consensus-team adjudication / targeted test.
- SIGTERM amplifier: drop(runtimes) kills the finalizer (tokio task) while the consensus core
  (dedicated thread) keeps committing and flushing during the drop window → the
  persisted-but-unfinalized commit suffix GROWS, exactly the population that gets re-finalized
  on restart. A hard kill stops both simultaneously (small suffix).

Result B (congestion/deferral): SYMMETRIC — verified. Tracker rebuilt fresh per commit from
quarantine∪DB debts stamped (producing_round, debt); decay = saturating_sub(budget * (cur -
stored - 1)), pure function, no rounding, applied once per load (consensus_quarantine.rs:970-978).
Deferred map round-trips via deferred_transactions_with_aliases_v3; DeferralKey ordering
deterministic. commit_budget's only dynamic input estimated_commit_period is rebuilt by the
prior-commit replay window, which provably equals the ring-buffer window
(sui-protocol-config lib.rs:2593-2601). Surface B eliminated.

Result C (impure-read sweep): NO new impure read feeding the CCP. Verified: d2 hashes ONLY
estimate_us (single observe_indirect_state call site, shared_object_congestion_tracker.rs:143);
cancelled-txn assignments use deterministic sentinels + tx-derived initial versions (insulated
from the live-store read in get_or_init_next_object_versions); execution_time_observations
table written in the SAME DBBatch as last_consensus_stats (consensus_quarantine.rs:300-303,
399-400); SystemTime in gather_commit_metadata is metrics-only; finalized_transactions_cache
not referenced in the commit path; handle_close_epoch / should_defer / init_randomness /
create_pending_checkpoints all quarantine-consistent. Residual: estimator base reads live
SuiSystemState EXECUTION_TIME_ESTIMATES field (authority_per_epoch_store.rs:1509-1510) —
epoch-stable under normal invariants.

CORRECTION: CoreThread is a tokio task (spawn_logged_monitored_task, core_thread.rs:195), not
a std thread — it dies with drop(runtimes) like the finalizer. The persisted-but-unfinalized
commit suffix instead comes from steady-state design: finalizer buffers ≥3 leader rounds
before finalizing while commits are flushed to the commits CF early (per-proposal flush).

## ROUND 3 — view-independence challenge (user)
Q: wouldn't the rejected-set asymmetry fork live validators under normal conditions?
A: Yes, IF vote-view differences could flip outcomes — so the design must make outcomes
view-independent: direct-reject needs a 2f+1 quorum; direct-accept is gated on a leader-
certificate quorum whose causal histories contain the deciders' votes (quorum intersection ⇒
no reject quorum can form); pending txns resolve indirectly on committed blocks (identical
everywhere); GC-conservatism (commit_finalizer.rs:282-333) forces uncertain evidence onto the
pending path. Under that lemma, replay's thinner view is just another legal view → canonical
outcome → surface exonerated. FINDING DOWNGRADED to conditional.
Recovery remains the place a lemma gap would manifest even if live never shows it:
- live views converge (near-identical block receipt + certificate gating), so a broken lemma
  is almost never exercised live; recovery view is an outlier (only flushed blocks; only
  committed votes until racy recover_blocks_after_round lands).
- re-finalized commits use the degraded remote prerequisite (decided_with_local_blocks=false,
  commit.rs:494): "some committed leader certificate + 3 commits" vs live's local
  certificate-quorum check.
- suspect reconciliation: direct-accept vs indirect-reject-at-depth
  (try_indirect_reject_pending_transactions_in_first_commit, commit_finalizer.rs:620-647);
  proof obligation: any-view direct-accept ⇒ committed accept-cert quorum within depth.
  Missing uncommitted-block votes are silent (the :334-335 panic only covers committed blocks).
Discriminating incident checks:
1. finalized_commits row for the diverged commit predating restart? (theory predicts absent)
2. replayed rejected set vs certified checkpoint accepted contents diff? (predicts mismatch)
3. if exonerated → remaining suspect is premise violation: consensus-store WAL tail loss
   across the upgrade (host replacement / pod migration / unclean host reboot — page cache
   does NOT carry over machines; fsync is off by default).

## ROUND 4 — user lead: handle_vote_transaction availability checks
Proposed: torn A/B cut changes vote outcomes on replay (input version no longer available).
Verified facts:
- Recovery DOES re-cast own votes: recover_and_vote_on_blocks re-runs block_verifier.vote →
  SuiTxValidator::vote_transaction → handle_vote_transaction, because "own votes on blocks are
  not stored" (transaction_vote_tracker.rs:93-135). Called per replayed unfinalized commit
  (commit_observer.rs:233) and for all stored blocks above GC (recover_blocks_after_round).
- handle_vote_transaction (authority.rs:1152-1210): executed-guard (is_recently_finalized
  in-memory cache + executed_transactions_to_checkpoint table) → accept; else exact-liveness
  owned-input check validate_owned_object_versions (must exist + version/digest match,
  object_locks.rs:92-105).
- BLOCKING ORDER: insert_finalized_transactions is a DIRECT epoch-DB write
  (authority_per_epoch_store.rs:2015-2020, immediate batch.write) issued in the
  FinalizeTransactions stage BEFORE the same checkpoint's A-write
  (checkpoint_executor/mod.rs:672 vs :436-451). Both WAL'd; issued writes survive process
  kill. So effects-durable ⟹ executed-marker-durable. The A/B tear can only produce
  marker-without-effects (benign: guard accepts, replay re-executes), never
  effects-without-marker. The primary flip (own-executed tx re-voted reject) CANNOT occur.
- Residual re-vote flips that DO survive:
  a. equivocation losers (inputs consumed by a different durably-executed tx): accept→reject
     flip, but excluded from costing deterministically either way (rejected-skip
     consensus_handler.rs:2575 vs lock-drop :2714-2739 both precede costing) → CCP unaffected.
  b. pipelined txs whose parent hadn't executed locally (exact-liveness rejects missing
     objects): flips both directions on pure timing — happens live between validators
     constantly; protocol must already tolerate sub-quorum vote perturbations.
- Net: the lead makes "replay vote view ≠ live vote view" GUARANTEED (own votes recomputed
  against different state), strengthening Round 3's crux: everything rests on finalization
  outcomes being invariant to sub-quorum vote perturbations for re-finalized commits
  (direct-accept vs pending→indirect-reject-at-depth). If that lemma has a gap, recovery
  re-voting is the trigger population (needs equivocation/pipelined traffic — present in
  production, rare in tests).
- Cheap probe: failpoint that flips one vote during recovery on an unfinalized-suffix commit;
  check whether the re-finalized rejected set diverges from the stored/canonical one.

## ROUND 5 — quorum-threshold correction (user) + repro test
- Correction to Round 4's "sub-quorum votes can't matter": at the quorum boundary every vote is
  pivotal. Finalization certificates require a stake quorum (>2/3); a tx whose accept support
  drops from quorum to quorum-minus-one on recovery flips from finalized to
  pending→indirect-rejected. With 3 equal-weight validators quorum = 3, so ANY single
  recovered-vote flip (Round 4's residual flips: equivocation losers, pipelined txs with
  locally-missing parents, exact-liveness re-votes) crosses the threshold. My earlier
  dismissal was wrong — one node's re-vote CAN change its local finalization outcome, and at
  small committees it changes it deterministically.
- Repro test added (commit 521daa6515):
  - New failpoint `after-commit-transaction-outputs` in checkpoint_executor/mod.rs between the
    perpetual-store batch write (A) and handle_finalized_checkpoint (B).
  - New simtest `test_simulated_load_crash_between_checkpoint_commit_and_finalization`
    (crates/sui-benchmark/tests/simtest.rs): 3-validator committee (equal stake; quorum = all
    3), kills validators at the failpoint with p=0.05 via handle_failpoint, 120s load.
    Note init_test_cluster_builder enables synthetic execution time injection, so the
    estimator/CCP surface is active.
  - Expected on current code if the mechanism is real: fork panic (CachedVersionMap fatal /
    checkpoint fork detection) on a restarted validator.

## CONCLUSION (round 2)
Every CCP input is verified replay-symmetric EXCEPT the rejected-transaction set of commits
that were persisted but never finalized before the crash. Those are re-finalized on restart
from a different reject-vote view than live (committed-sub-dag blocks only, vs all received
blocks live; recover_blocks_after_round races the finalizer). A flipped accept/reject decision
changes the accepted tx set (different estimate_us stream folded into d2) and/or flips an
ExecutionTimeObservation tx (shifting estimator state itself) → "differing execution time
estimates" → divergent CCP → collision with the state-synced canonical CCP already on disk.
Open question for consensus team: is re-finalization guaranteed outcome-equivalent to live
finalization (quorum-intersection argument), given (a) the pending-vs-direct path difference
when live saw sub-quorum reject votes from uncommitted blocks, and (b) the vote-recovery race?

### Fix directions (not implemented)
1. Handle SIGTERM properly: orderly stop (stop consensus → drain checkpoint executor pipeline
   through its B-write → then exit), instead of bare drop(runtimes).
2. Guard the replay: when re-processing a commit at/below the executed-checkpoint watermark,
   compare the regenerated CCP digest to the executed one — skip if equal, fatal with a clear
   diagnostic (not a version-collision panic) if not.
3. Add a crash failpoint between (A) and (B) in execute_checkpoint (one exists only between B
   and C, mod.rs:506) and a simtest for it; add a SIGTERM-style shutdown simtest; consider a
   cross-binary replay determinism test.

## Findings

### Reader 5 (checkpoint executor / CCP race) — done
- `last_consensus_stats` (consensus replay resume pointer) is NOT persisted when a commit is
  handled. It sits in the in-memory `ConsensusOutputQuarantine` and is only flushed in
  `ConsensusOutputQuarantine::commit_with_batch` (consensus_quarantine.rs:589-670), gated on:
  (1) a locally-built checkpoint summary at/below `highest_executed_checkpoint`, and
  (2) `checkpoint_queue_drained` (introduced by commit `1ad56a6b81`, #25442).
- Checkpoint executor (runs on validators too) executes synced checkpoints incl. CCP, assigns
  shared versions **from effects** (`assign_versions_from_effects`), commits object writes, but
  never advances `last_consensus_stats` / `consensus_message_processed` / `next_shared_object_versions`
  (only first-touch init from current object-store version, authority_per_epoch_store.rs:2078-2161).
- So: crash after checkpoint-executor committed commit N's CCP writes but before quarantine flush
  → restart → `last_processed_subdag_index()` < N → consensus replays commit N → regenerates CCP.
  If regenerated CCP differs (different digest → not deduped), it writes an output object at an
  already-existing version → `CachedVersionMap::insert` fatal (cache_types.rs:50-62, fires on `>=`).
- Quarantine explicitly anticipates "state sync running ahead of consensus" (consensus_quarantine.rs:550)
  but only for committing quarantined data, not for suppressing replay.
- Open question: which CCP input actually diverges on replay (candidates: lost quarantine state —
  congestion debts / deferred txns / execution-time observations — feeding cancelled-txn version
  assignments; timestamp logic). Also note replay-window comment consensus_handler.rs:210-214:
  "any deviation causes an immediate fork".
- Relevant commits: `1ad56a6b81` (#25442 drain-boundary logic), `efd3e7e374` (#21310 replay window).

### Reader 2 (RocksDB durability) — done
- NO store disables WAL. All stores (perpetual, epoch, checkpoint, consensus) use default
  WriteOptions: WAL on, sync off (`SUI_DB_SYNC_TO_DISK` off by default,
  typed-store rocks/mod.rs:61-74, 1474-1482). So every *issued* write survives process SIGKILL
  via OS page cache; only machine/power loss can lose WAL tail.
- No explicit flush hooks anywhere; durability at shutdown is implicit via RocksDB clean-close
  (Drop → cancel_all_background_work; avoid_flush_during_shutdown unset → memtables flushed).
- Three separate DBs, three WALs, written non-atomically at checkpoint execution
  (checkpoint_executor/mod.rs:415-514): (A) perpetual batch incl. CCP outputs +
  highest-committed watermark, (B) epoch batch = quarantine flush (last_consensus_stats etc.),
  (C) checkpoint store watermark. `fail_point!("crash")` exists between B and C.
- WritebackCache dirty set persists ONLY via checkpoint-executor commit_transaction_outputs;
  no shutdown flush; executed-but-uncheckpointed effects lost on any crash (recomputable).
- **Critique (mine)**: since all issued writes survive SIGKILL via page cache, the
  "epoch DB write lost while perpetual flushed" story doesn't hold for process-kill —
  the persisted cut at SIGKILL equals whatever was issued, graceful or not. The real
  SIGTERM-specific difference must be *which components keep issuing writes during the
  drawn-out graceful window* (e.g. consensus/handler stopped early while state sync +
  checkpoint executor keep executing synced checkpoints for many seconds), widening the
  "checkpoint executor ahead of local consensus processing" gap far beyond anything tests
  produce. Pending Reader 1 (shutdown ordering) to confirm.

### Candidate root-cause mechanism (synthesis so far)
1. Node is (or becomes, during graceful shutdown) behind on local consensus-handler processing
   while state sync + checkpoint executor execute certified checkpoints containing commits
   (P, N] — including their canonical CCPs — writing object versions to perpetual DB.
2. P (last_consensus_stats) never advances for those commits (no local handler processing →
   no quarantine outputs → no flush). Node killed.
3. Restart: consensus replays/produces commits > P; handler fully re-processes them and
   regenerates CCPs. Regeneration is only safe if rebuilt state matches canonical state at
   each commit. Divergence vectors:
   - `get_or_init_next_object_versions` lazily inits from CURRENT perpetual-store object
     version — polluted by synced execution up to N (authority_per_epoch_store.rs:2078-2161).
   - checkpoint-executor path itself writes first-touch init entries into
     next_shared_object_versions_v2 reflecting versions at N, not at the replayed commit.
   - execution_time_observations table / debts / deferred state at P vs. what canonical
     processing of (P, replayed-commit] expects (should be replay-consistent if batch-atomic
     with P — needs verification).
4. Divergent CCP (different version assignments and/or additional_state_digest) → different
   digest → not deduped → writes shared object at already-existing version →
   CachedVersionMap::insert fatal.
- Replay boundary P = epoch store `last_consensus_stats.index.sub_dag_index`; consensus replays
  from P - W (W = consensus_num_requested_prior_commits_at_startup). Commits ≤ P →
  `handle_prior_consensus_commit` (observe-only, rebuilds ring buffer); > P → full
  `handle_consensus_commit`, CCP regenerated. No other skip; nothing consults
  highest-executed-checkpoint to suppress re-processing (consensus_handler.rs:3157-3170).
- Consensus side: CommitFinalizer flushes DagState to consensus store BEFORE sending commit to
  the handler (commit_finalizer.rs:141-144). Consensus store writes use default WriteOptions —
  WAL-backed, not fsync'd. Survives SIGKILL; only machine/power loss loses WAL tail.
- `recover_and_send_commits` re-loads replayed commits verbatim from the consensus store —
  never recomputed. So pure "store ahead of P" replay is content-identical.
- Note: quarantine `write_to_batch` writes last_consensus_stats AND execution_time_observations
  / debts / deferred / next_versions in ONE batch → those tables should be mutually consistent
  at P. Estimator state at commit N = f(stored obs, observations in commits ≤ N), and full
  replay of (P, N] re-applies them ⇒ replay *should* be deterministic if consensus store intact.
- Divergence therefore requires one of:
  a. consensus store lost/re-derived commit N (different timestamp clamp via prior-commit chain,
     different leader schedule from rebuilt scoring_subdag, different GC/sub-dag membership) —
     linearizer.rs:125-156, dag_state.rs:120-260; amnesia/boot-counter gating at
     consensus_manager/mod.rs:322-344;
  b. epoch-store-side node-local state at replay differing from original run (see Reader 4);
  c. epoch store P persisted AHEAD of consensus store content (store behind P) — should be
     impossible given flush-before-send, unless flush durability differs (WAL vs no-WAL asymmetry
     — pending Reader 2).

### Reader 4 (CCP construction determinism) — done
CCP V4 fields (consensus_handler.rs:416-475; transaction.rs:4132-4152):
- Pure functions of commit content (cannot diverge on replay): epoch, round,
  sub_dag_index (always None in V4), commit_timestamp_ms (commit ts clamped to epoch start —
  no median/local-clock logic), consensus_commit_digest.
- **Divergence candidates** (depend on accumulated node-local state):
  1. `consensus_determined_version_assignments` — cancelled txns + versions. Depends on:
     SharedObjectCongestionTracker (tx_cost = ExecutionTimeEstimator::get_estimate),
     initial object debts (congestion_control_*_object_debts tables via quarantine),
     deferred_transactions_with_aliases_v3, next_shared_object_versions_v2, DKG/randomness state.
  2. `additional_state_digest` = fold(d1, d2):
     - d1 = CommitIntervalObserver ring buffer of last-N commit timestamps — deterministic,
       reconstructed via `handle_prior_consensus_commit` replay window. Low risk.
     - d2 = IndirectStateObserver — **directly hashes per-tx ExecutionTimeEstimator estimates**
       (shared_object_congestion_tracker.rs:143). If estimator state differs on replay, the CCP
       digest forks even with no cancellation change.
- ExecutionTimeEstimator durability: observations arrive via consensus, staged in quarantine,
  persisted to `execution_time_observations` table only on quarantine flush (= checkpoint
  executed + queue drained). Rebuilt at startup from SuiSystemState (prev epoch) + that table.
  `handle_prior_consensus_commit` does NOT re-apply observations — only full replay does.
- All the relevant tables (last_consensus_stats_v2, consensus_message_processed,
  next_shared_object_versions_v2, deferred txns, debts, execution_time_observations) are written
  ONLY by `ConsensusOutputQuarantine::write_to_batch`, in a separate DB batch from the object
  writes done by checkpoint execution → non-atomic boundary = torn-shutdown surface.
- Most likely fork mechanism: estimator/debts/deferred state at replay time differs from state
  that produced the canonical CCP → d2 and/or cancellation set differ → different CCP digest →
  not deduped → writes shared-object versions colliding with the state-synced CCP's writes.
