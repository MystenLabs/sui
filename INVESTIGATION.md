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

## ROUND 6 — repro attempts and the 3-validator topology dead end
Run log (all with failpoint `after-commit-transaction-outputs` killing validators):
1. 3 validators, 10s epochs, p=0.05, slow restarts → starved (0 successful txs; quorum=3
   means any dead node halts the network; kills landed during setup/epoch changes).
2. 3 validators, 60s epochs, 30s arming delay, p=0.02 → PASSED (load flowed, no fork).
3. Same + fast restarts (0.5-2s, new handle_failpoint_with_restart_range_ms) → PASSED.
4. Same + conflicting-transfer workload (reject votes), 10 seeds (MSIM_TEST_NUM=10 SEED=2)
   → all PASSED.

Topology insight (why 3-of-3 cannot reproduce this): the fork requires commits that are
locally UNFINALIZED (→ re-finalized on restart) yet have canonical effects DURABLY on disk
(→ collision target). Durable effects for unfinalized commits can only come from executing
STATE-SYNCED certified checkpoints. With quorum = full committee: (a) checkpoint
certification requires every validator's signature, so anything certified was already
locally finalized and handled (replayed verbatim — safe); (b) while the victim is down the
rest cannot advance, so there is nothing new to sync on restart. The dangerous population
cannot form.

Switch to 4 validators (quorum 3): single vote still pivotal (3 accepts → 2 = below quorum,
per user's threshold point), but the network progresses during downtime → restarted node
executes state-synced checkpoints for commits it never finalized → a second crash DURING
catch-up (short grace period, 1-5s) lands the torn cut on exactly the dangerous population.
Long downtimes (5-15s) let the network get ahead. Grace range parameterized in
handle_failpoint_with_restart_range_ms.

## ROUND 7 — re-finalization exercised; vote-flip theory DEAD
Instrumented runs (markers: "killing node" / "Starting to recover unfinalized" /
"recovery-vote-flip: injecting"):
- Without finalizer delay: 14 kills, 8 flips, **0 unfinalized recoveries** — commits finalize
  ~immediately when decided with local blocks (finalized_commits row lands with the commit),
  so the dangerous population never forms; earlier green runs proved nothing.
- Added `commit-finalizer-delay` failpoint (commit_finalizer.rs run loop; registered with
  delay_failpoint(500..2000, 0.1)): commits persist via proposal flushes long before their
  finalized rows, AND the delayed node's checkpoint signing lags so state sync runs ahead —
  both fork ingredients on the same node.
- With delay, 5 seeds: 88 kills, **27 unfinalized-commit recoveries (re-finalization
  exercised)**, 19 vote flips — ALL PASSED. No fork.

Why a self vote flip can never flip finalization (quorum arithmetic, corroborated):
canonical accept ⇒ accept-cert quorum (≥2f+1) in committed blocks ⇒ canonical reject stake
≤ f; + own stake ≤ 2f < 2f+1 ⇒ direct-reject unreachable. Indirect resolution reads only
committed-block certificates (immutable; own pre-crash accept votes already committed).
Flip only moves a tx direct-accept → pending → indirect-accept. Same outcome.
⇒ The handle_vote_transaction/recovery-re-vote mechanism is NOT the root cause.

## Remaining live hypotheses (in order)
1. Premise violation: replayed commit CONTENT differs — consensus store losing its
   un-fsync'd WAL tail across the upgrade restart. Impossible for same-host process kill
   (page cache); possible with host replacement, unclean host reboot, container/PV
   migration, or upgrade procedures that touch the consensus DB (snapshot restore wipes
   it?). Lost/re-derived commits change which ExecutionTimeObservation txns are in which
   commit → estimator state differs → "differing execution time estimates" → CCP fork.
   ACTION: get incident operators' upgrade procedure + whether host/pod moved; check
   incident consensus store for commit-content mismatch vs peers.
2. Estimator-table/P atomicity hole via a path not yet found (main path verified atomic).
3. Incident-data discriminators still decisive: finalized_commits row for the diverged
   commit; rejected-set diff vs certified checkpoint; CCP field diff (assignments vs d2).

## ROUND 8 — execution-time surface, mid-epoch lens (user redirect)
User: bug occurs at ANY time, not only epoch transitions; Antithesis uses real code paths →
epoch-boundary stored-obs pollution rejected as primary cause (still a latent landmine:
get_stored_execution_time_observations has no epoch guard, authority_per_epoch_store.rs:1509).

Verified solo this round (all consistent):
- No pruning/deletion on execution_time_observations table.
- Observation dedup key = ConsensusTransactionKey::ExecutionTimeObservation(AuthorityName,
  generation) (messages_consensus.rs:253) → same-(authority,gen) duplicates can't reach the
  estimator or the table twice → (generation, authority) table-key overwrite hazard dead for
  processed txns.
- Quarantine debt/next-version maps are RefCountedHashMap (consensus_quarantine.rs:991-1021):
  insert bumps refcount + takes newest value; flush-remove decrements → flushing an older
  output cannot clobber a newer quarantined value. Clobbering theory dead.
- Stale debt rows self-correct: payable debt implies residual ≤ budget, so stale (round, debt)
  decays to 0 by the next round — snapshot+decay design is deterministic regardless of which
  commits touched the object. Debts fully verified (again).
- Formal argument: estimator@replay == estimator@live given (a) table rows == processed
  observations ≤ P (unconditional staging, atomic flush with P), (b) max-gen-wins rebuild ==
  sequential application (order-independent due to strict > check), (c) replayed commits
  deliver identical observation txns (dedup state quarantine-consistent).
- Exotic asymmetry found (Byzantine-only): a generation==0 observation live-creates an entry
  with median Some(ZERO) (flag default_none_duration_for_new_keys=false) and never updates it,
  while rebuild's final median pass recomputes → None → default_duration. live=0 vs
  rebuild=default. Requires gen-0 observation — honest senders use SystemTime micros. Noted.

Dispatched adversarial auditors:
A. ExecutionTimeObserver end-to-end (local observations, generations across restarts,
   indebted objects → any non-consensus feedback into commit processing).
B. Estimator load/store/rebuild with assume-it's-broken framing + CURRENT mainnet values and
   introduction versions of default_none_duration_for_new_keys / enable_observation_chunking /
   ExecutionTimeEstimateParams (recent flag flip would match incident timing).
C. Full diffs of 44791e888f / 3ab366bc03 / b9149cbf0b (commit-handler pipelining) — hunting a
   live-vs-replay interleaving break of the single-threaded model.

### Round 8 results — execution-time surface exhaustively CLEAN on current mainnet
A. Observer: all-clear. Strictly one-way producer (local timings/utilization/indebted/
   generations affect only WHICH observation txns get submitted). No state read by commit
   processing. Theoretical (gen,source) table collision doubly dead (rate limiter + dedup).
B. Estimator load/rebuild: all-clear for current config. Key protocol facts:
   - default_none_duration_for_new_keys: false at v84 (mainnet ETE enable, patch to 1.48),
     flipped TRUE at v88 — the flip REMOVED the only real asymmetry (gen-0 default-median
     Some(ZERO)-vs-None divergence exists only with flag=false + Byzantine gen-0 sender).
   - Chunking ON at v102 (Some(18)); merge_sorted_chunks deterministic, epoch-stable.
   - stored_observations_limit 20(v84)→18(v94)→180(v102); median threshold 3334 from v86.
   - last_consensus_stats + execution_time_observations in same WriteBatch (re-verified).
   - Only writer = process_observations_from_consensus; scheduling only calls &self
     get_estimate (no unpersisted mutation during CCP construction).
C. Pipelining diffs (44791e888f, 3ab366bc03, b9149cbf0b): all-clear. Only pure BCS
   deserialization made concurrent (capacity-2 FIFO); filter/dedup/estimator/congestion all
   remain on the single Stage-2 task in identical intra-commit order; commits strictly
   serialized. try_lock().expect guards would panic, not diverge.
   INCIDENTAL: admin accessors get_estimated_tx_cost / get_consensus_tx_cost_estimates take
   the estimator mutex via .lock().await (authority_per_epoch_store.rs:3352, 3362) — an
   admin call timed against commit handling PANICS the handler on try_lock().expect. Crash
   hazard, not divergence. Worth fixing separately.

## ELIMINATION CONCLUSION (round 8)
Every component that computes execution-time estimates is now verified replay-deterministic
under the premises {epoch tables atomic at P; replayed commits byte-identical; same epoch;
same binary}. If incident estimates truly differed, an INPUT differed:
  (i) replayed commit content ≠ original (consensus store integrity: WAL tail, or commit
      re-derivation/commit-sync differences), or
  (ii) the costed transaction set differed (rejected-set of a commit never handled locally —
      re-finalization; our synthetic probe showed robustness but only for injected rejects
      under one choreography).
Next discriminating steps (need incident data):
  1. How exactly was "differing estimates" observed in the incident (which logs/artifacts)?
  2. Extract the diverged commit from the incident consensus store; byte-compare content +
     rejected set against a healthy peer's copy. Decides (i) vs (ii) vs state.
  3. Instrument now for the next occurrence: at CCP construction log (commit index, hash of
     estimator consensus_observations, costed-tx count, folded estimate stream hash) — one
     log line pinpoints which component diverges.

## ROUND 9 — commit-processing delay + proper seed search (user redirect)
Rationale: checkpoint-executor-ahead-of-local-consensus-handler is a necessary precondition
for the fork (canonical CCP effects durable via state sync while local resume pointer P
lags). Stalling handle_consensus_commit widens this window directly, cheaper than
choreographing crashes/restarts to induce it.
- Added `fail_point_async!("handle-consensus-commit-delay")` at the very top of
  handle_consensus_commit (consensus_handler.rs), before any protocol-config asserts.
  Commit 91f8916e31.
- Wired into EXISTING test_simulated_load_reconfig_with_crashes_and_delays (not our new
  test) via register_fail_point_async(delay_failpoint(500..3000, 0.05)) — composes with its
  existing crash failpoints (batch-write-before, crash, highest-executed-checkpoint, etc.)
  rather than needing separate choreography.
- Smoke tests (p=0.01 then p=0.05, single seed each): both passed, 84s.
- Proper tool: scripts/simtest/seed-search.py (parallel across cores, not sequential
  MSIM_TEST_NUM). Correct invocation: --test simtest (binary name, NOT "sui-benchmark::simtest"
  — cargo test target names don't include package prefix).
- Launched: 200 seeds, test_simulated_load_reconfig_with_crashes_and_delays, log-dir capturing
  per-seed logs + failures.ndjson.
- RESULT: 200/200 PASSED. Reachability confirms real exercise of the suspect machinery
  (not just idling through the delay): "successfully loads stored execution time
  observations", "receives some valid execution time observations", "cancelled
  transactions", "cancelled randomness-using transaction", "cancelled non-randomness-using
  transaction" all reached across the sweep. Combined with existing crash failpoints
  (batch-write-before, crash, highest-executed-checkpoint) and DKG-failure injection
  (rb-dkg 6%).
- Conclusion: widening the state-sync-ahead window via direct handler delay, at p=0.05 over
  200 seeds with active estimator/congestion/cancellation traffic and real crashes, still did
  not reproduce the fork. This is now the strongest negative evidence yet that
  "checkpoint-executor-ahead-of-local-consensus" alone (even combined with torn-cut crashes
  and reject-vote-driven re-finalization, per Round 7) is insufficient — some other
  ingredient is still missing, OR the reproduction requires a specific interleaving/scale
  this harness's crash timing/probabilities don't hit, OR (per Round 8 elimination) the true
  cause lies outside anything a same-binary, same-host, single-process-kill simtest can
  produce (e.g. consensus-store content loss across host-changing upgrades).

## ROUND 10 — real local-cluster reproduction attempt (user redirect)
Moving from simtest to a real 4-validator local cluster with actual OS-process sui-node
instances, so restarts hit genuine signal/runtime-drop/OS-scheduling behavior rather than
msim's virtual clock and cooperative scheduling — a categorically different (and closer to
production) test of the same hypothesis.

Setup (in scratchpad/localnet/):
- `sui genesis --committee-size 4 --epoch-duration-ms 600000` generates network.yaml,
  genesis.blob, per-validator config files (127.0.0.1-PORT.yaml), fullnode.yaml, keystore.
  Confirmed: db paths/ports already unique per validator (keyed on pubkey) — no manual editing
  needed; sui genesis already splits network.yaml into individual validator config files.
- Protocol version: MAX_PROTOCOL_VERSION=129 unconditionally sets ExecutionTimeEstimate mode
  at v84 for ALL chains (not just Mainnet — verified the v84 block in
  crates/sui-protocol-config/src/lib.rs applies per_object_congestion_control_mode outside
  the `if chain == Mainnet` guard), so local genesis (Chain::Unknown) gets ETE by default.
- `run_experiment.py`: launches 4 validators + 1 fullnode as real subprocesses
  (CRASH_ON_PANIC=1 so fatal! panics escalate to process exit(12) instead of just killing a
  tokio task silently), RUST_LOG tuned to surface execution_time_estimator debug logs
  ("sharing new execution time observation") and checkpoint/consensus info logs. Runs `stress`
  bench pointed at the live cluster (LocalValidatorAggregatorProxy, needs one fullnode RPC for
  reconfig) with --shared-counter 100 --num-shared-counters 1 (single hot object, forces
  serialization + congestion) --transfer-object 0. Then repeatedly: let validator-3 run 8-20s,
  SIGTERM, wait a random 0.2-4s grace period, SIGKILL if still alive, immediately relaunch
  pointed at the same config/db (mimics real upgrade-restart timing incl. graceful-then-forced
  kill). Scans all node logs every cycle for fatal!/panic/monotonic-version patterns.
- primary-gas-owner-id quirk: despite the flag name, in local (non-fullnode-execution) mode
  stress treats it as a SuiAddress (ObjectID::from_hex_literal(..).into()) and searches
  genesis objects for that address's gas coin — use `sui client active-address`, not an object
  id.
- Build in progress (debug profile): sui, sui-node, stress.

### Round 10 execution notes
- User challenge (correct): confirm ETE observations are ACTUALLY sent, not just enabled —
  congestion mode being on doesn't guarantee the observer's share-threshold (object must be
  "overutilized": accumulated execution time on the object exceeds target_utilization% of
  wall-clock time since last measurement, execution_time_estimator.rs:336-413) is ever crossed.
- First attempt found ZERO observations after 30s warmup — investigated rather than assumed
  benign: `stress` had actually crashed instantly on a CLI parse error. Root cause:
  `--fullnode-rpc-addresses` uses `num_args(1..)` with no terminator, so a bare following token
  (`bench`) gets greedily consumed as a second URL, eating the subcommand entirely ("error:
  unexpected argument '--shared-counter' found"). Fix: use `--fullnode-rpc-addresses=URL`
  (`=` form limits clap to one value regardless of num_args). Confirmed via `stress help bench`
  that --shared-counter/--num-shared-counters etc. are the correct flag names.
- Second attempt (fixed CLI): real load flowed (TPS~31, single shared counter, p50 latency
  837ms indicating real contention on the object) and **3 execution-time observation shares
  were confirmed within 60s** — answers the user's question affirmatively once real congestion
  exists. (Note: achieved TPS collapsed to 0 after ~15s with no_gas climbing to 2000 — the
  bench driver's gas-object pool likely got exhausted/locked up by the single-hot-object
  contention pattern; worth tuning gas-request-chunk-size / primary gas supply for longer runs.)
- Same run hit an UNRELATED crash within ~8s of load starting, before any kill/restart cycle:
  two validators (0 and 3) independently panicked at the identical location,
  writeback_cache.rs:756 ("object_by_id cache is incoherent for..."), inside a
  `cfg!(debug_assertions)` block (writeback_cache.rs:718, sibling checks at :277, :1510).
  Root-caused: this fires ONLY in debug builds — confirmed `[profile.release]` in root
  Cargo.toml does not set `debug-assertions = true` (unlike `[profile.simulator]`, which does).
  Very likely a TOCTOU race in the paranoid check itself (snapshots object_by_id_cache then
  separately re-reads dirty.objects/store non-atomically) under heavy concurrent access to one
  hot object — orthogonal to the CCP-replay investigation and irreproducible in real (release)
  validators or in Antithesis. NOT chased further; noted as a possible independent finding.
- Pivoted to `--release` build for sui/sui-node/stress (also gives `panic = 'abort'`, i.e. any
  panic — including our target `fatal!` — aborts the process immediately, no dependency on
  CRASH_ON_PANIC's telemetry-hook escalation).
- Release run with shared-counter workload: real throughput (~200-220 TPS sustained, 0 errors,
  in_flight staying low ~25-40 i.e. no backlog) but ZERO ETE observations after 60s. Root
  cause (not a harness bug this time): shared-counter's per-tx execution time is sub-ms, so
  even 200 sequential executions/sec on one object stays far below the observer's
  overutilization threshold (accumulated LOCAL execution time must exceed ~30-50% of elapsed
  wall-clock time since last measurement, execution_time_estimator.rs:336-413,364-379,463-465
  `overutilized()`). High serialized THROUGHPUT on a hot object ≠ high object UTILIZATION from
  the estimator's perspective — the latter only cares about actual time spent executing, not
  submission/serialization rate.
- Switched to the `slow` workload (crates/sui-benchmark/src/workloads/slow.rs /
  data/slow/sources/slow.move): explicitly designed for this — attaches an UNUSED mutable
  shared object purely "to activate congestion control" (slow.rs:92-97), while `bimodal()`
  does real CPU-bound work (alternates every 10s between allocating 10x10-byte and
  100x100-byte vectors). One shared `Obj` created once in `init()`, reused by every payload —
  guaranteed single-object hotness matching our max-contention goal. Confirmed via source read
  before relaunch (not assumed).
- Regenerated genesis fresh (release binaries). Result: **31 execution-time observation
  shares within 60s**, and the bimodal pattern is directly visible in the stress client's own
  latency stats (p99 767-1109ms during "slow" 10s windows vs 244-335ms during "fast" windows).
  Genuine, sustained ETE/congestion-control traffic confirmed — not merely enabled, actually
  triggered by real contention.
- Kill/restart loop against validator-3 (4-validator committee, quorum 3) with this live
  congestion background: 8-20s uptime, SIGTERM, 0.2-4s grace, SIGKILL if still alive,
  immediate restart pointed at same config/db. First long run hit a FATAL_PATTERNS false
  positive: bare regex `"fork"` matched a base64 digest substring inside a routine proposer
  log line ("...forkjwBs3ioU=..."). Fixed: tightened to `"fork detected!"` (the actual
  checkpoint-executor fork-detection log text, checkpoint_executor/utils.rs:169) and
  `"thread '.*' panicked at"` (matches any real Rust panic, superset of the removed bare
  pattern). Relaunched clean.

### Round 10 RESULT — real-cluster negative result, precondition confirmed exercised
~65 minutes, 212 kill/restart cycles on validator-3, spanning 5 epoch transitions (epoch 2→6).
Confirmed genuinely severe, sustained conditions throughout — this was not a quiet run:
- Real congestion: p99 latency spiked as high as 13.5s; 7727+ cancellation/congestion log
  events per validator (`cancelled`/`CONGESTED`/`expensive tx cost`).
- Real ETE traffic: ~500-550 "sharing new execution time observation" shares per validator.
- **Real state-sync-ahead-of-consensus catch-up, repeatedly**: validator-3 executed bursts of
  up to 42 already-certified checkpoints within a single second, multiple times across the
  run (23:16:59, 23:22:25, 23:33:30, 23:36:59, 23:54:45) — direct evidence the necessary
  precondition (checkpoint executor racing ahead of local consensus commit processing after
  restart) was genuinely and repeatedly exercised, not just theorized.
- One instance of the known epoch-boundary race: "change epoch tx has already been executed
  via state sync" (validator-3 only, once) — this is the ALREADY-HANDLED case noted in Round 1
  (`execute_change_epoch_tx`'s crash-recovery comment, mod.rs:989-996), not a fork.
- **Zero forks, zero fatal patterns, zero unexpected crashes** across the entire run.

This is the strongest negative evidence yet: real OS processes, real SIGTERM/SIGKILL (not
simulated), real congestion-driven execution-time-estimate traffic, real repeated state-sync-
ahead catch-up racing, sustained for over an hour across multiple epochs — and the fork does
not reproduce. Run left going in background for extended coverage; will report if it changes.

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
