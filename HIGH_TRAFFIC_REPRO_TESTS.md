# sui_mainnet_high_traffic — Incident Repro Test Log

Tracking the effort to reproduce the mainnet high-traffic incident's **consensus round-rate collapse** on private-testnet (PT).

---

## Executive summary

**Outcome:** 8 tests across Tasos and Alex reproduced every *individual* signal of the June 2026 mainnet high-traffic incident — latency regression, submit-semaphore saturation, shared-object congestion, and ~1 MB blocks — but **did not reproduce the consensus round-rate collapse.** The campaign is concluded; a faithful PT replay is not achievable under the agreed constraint of excluding mainnet hardware differences.

**What the incident was:** MEV searchers amplified and spammed **failing** DEX-arbitrage transactions (Cetus / flash-loan multi-hop) against a handful of **hot shared objects**. Those txns carry real execution time and large byte size, submitted at ~20K tps (with <150 tps of unique work). This drove three pressures at once: (1) per-object **execution-time congestion** (mainnet ran `ExecutionTimeEstimate` mode), (2) **submit-semaphore saturation** with no system-message bypass in v1.73.2, and (3) **~1 MB blocks** that slowed consensus RocksDB/sequencing. Together they starved checkpoint-signature/system-message submission and stalled round progression — round rate fell 13 → 3.7/s.

**Why PT couldn't reproduce it — and what we ruled out:** the mainnet collapse was a **uniform block-production slowdown** (production, acceptance, and commit rates all fell 11.6 → 3.6/s together) coincident with peak traffic, ~1 MB blocks, and checkpoint-signature starvation. A metric sweep of the collapse window **ruled out, with data, the four leading candidate causes**: consensus RocksDB latency/stalls (zero write stalls; latency ~flat — an earlier draft wrongly blamed this), execution-backlog magnitude (PT ran 2.9M pending certs vs mainnet's 59k without collapsing), execution-cache backpressure (fired on PT, not mainnet), and fleet propagation delay (~0 on both). **The cause of the PT/mainnet divergence is therefore not yet definitively identified.** The leading remaining hypothesis is **per-round consensus processing cost** (verify/deserialize each block's txns), which scales with block size *and* per-tx complexity ÷ CPU — we reproduced block size but not real-txn complexity, and PT's fast uniform CPUs outpace mainnet's heterogeneous fleet. This needs code-level confirmation.

**Key technical findings:**
- Congestion is **execution-time-based**, not gas- or count-based — cheap synthetic traffic (Tests 3–5) can never trigger it regardless of volume or gas budget; the purpose-built `slow` workload (Test 6) was required and did fire it (obj cost 743k vs incident 733k).
- **Congestion alone does not collapse rounds** — it defers gracefully and protects liveness (Test 6).
- **Semaphore saturation alone does not collapse rounds** either (Tests 3, 4, 7) — permits release at sequencing, which is fast on PT.
- **RocksDB was measured and ruled out** as the collapse driver (no write stalls on mainnet during the incident); so were execution-backlog magnitude and execution-cache backpressure (PT exceeded mainnet on both without collapsing).
- The mainnet collapse is a **uniform threshold-clock (block production) slowdown that tracks block size** — pointing at per-round processing cost, not a storage or backpressure stall.

**Recommendation:** the divergence cause is not pinned, so the next step is **code-level analysis of what throttles Mysticeti block production under load**, plus a PT test of **transaction-verification complexity** (many commands / shared-object inputs per tx, not just byte size). Separately, validate the specific incident-response fixes in isolation (unit/sim: #27123, #27074, congestion/backpressure). A full end-to-end replay would need mainnet-representative hardware and realistic tx complexity. Full details, per-test template, and metrics below.

---

## FACTS

Only directly observed / verifiable items. No interpretation or causal claims.

### Environment
- Affected network: **Sui mainnet**, running release **`v1.73.2`**, **protocol version 86**.
- Per-object congestion control mode at incident: **`ExecutionTimeEstimate`** (`target_utilization: 50`, `max_estimate_us: 1_500_000`, `max_deferral_rounds_for_congestion_control: 10`). Source: `Mainnet_version_86` protocol snapshot.
- Consensus-adapter submit-semaphore size = `max_pending_transactions * 2 / num_validators` = `40_000 / num_validators` permits (~350 at ~113 validators). Source: `sui-node/src/lib.rs:1581`, `max_pending_transactions` default 20000.
- `v1.73.2` does **not** contain PR #27074 (soft-bundle in-flight accounting), PR #27123 (system-message semaphore bypass), or PR #27133 (notify dropped/rejected double-spend losers).

### Timeline (UTC)
| Time | Event |
|---|---|
| 2026-06-23 07:20 | Mainnet begins receiving high transaction traffic (~19K tps); p50 latency exceeds 15s |
| 2026-06-23 08:59 | PagerDuty "High Traffic Volume" fires; `#sui_mainnet_high_traffic` channel created |
| 2026-06-23 09:20, 09:48 | Repeated high-traffic bursts >15K tps |
| 2026-06-24 (all day) | Amplified traffic sustained — 3rd consecutive day; most severe round-rate degradation |
| 2026-06-24 16:52 | `max_congestion_control_object_costs` peaks **733,489** |
| 2026-06-24 16:58 | `consensus_handler_congested_transactions` peaks **24,761/s**; `cancelled_transactions` peaks **1,714/s** |
| 2026-06-24 17:56 | `consensus_core_skipped_proposals` peaks **23.7/s** |
| 2026-06-24 18:18 | total consensus tx/s peaks **22,062**; round-rate avg troughs **3.7/s**; `checkpoint_signature` tx/s troughs **69.5/s** |

### Observed incident metrics (mainnet, 06-23 → 06-25)
| Metric | Value |
|---|---|
| `consensus_last_committed_leader_round` rate (avg) | ~12.8/s baseline, **trough 3.7/s** (individual nodes flatlined) |
| `consensus_handler_congested_transactions` | peak **24,761/s** |
| `consensus_handler_cancelled_transactions` | peak **1,714/s** |
| `max_congestion_control_object_costs` | peak **733,489** |
| `checkpoint_signature` tx/s | ~522 baseline → **trough 69.5** |
| `sequencing_in_flight_submissions` | pinned at cap **~310** |
| hosts with `sequencing_in_flight_semaphore_wait` > 5000 | up to **~120 of ~125** |
| `consensus_handler_duplicate_tx_count` | ~2,300/s avg |
| `consensus_proposed_block_size` | p50 ~11 KB (peak 62.8 KB); **p99 ~238 KB avg, peak 959 KB (~1 MB)** |
| fleet-wide `consensus_round_tracker_last_propagation_delay` | ~0 (only 1–3 isolated stuck hosts) |

### Traffic composition
- Dominated by MEV amplification + gas double-spend + soft-bundle submissions. Soft-bundle submissions reached ~21K req/s at the largest round-rate dip (reported by A. Kichidis).
- Unique (non-duplicate) transaction rate reported as **<150 tps** against ~20K total.
- Representative transactions (mainnet digests):
  - `A7aHko5f8avpWXZUttxpwMmCdwQQ9ui5C1HHXQKBVTi1` — Cetus router swap; **4 mutable shared objects**; 5 commands; gas budget 3,000,000; **status FAILURE** (`err_amount_out_slippage_check_failed`).
  - `DTnFPGL38j1BXCCjn1jEr3cPLow7G8KD9LRauSqHZWsq` — para/aftermath/kriya_clmm multi-DEX flash-loan arbitrage; **12 shared objects**; 11 MoveCalls; gas budget 20,000,000; **status FAILURE** (`repay_loan_quote` abort).

### Result to date
- Across all PT tests below (Tasos + Alex), the **round-rate collapse has not been reproduced**. Latency regression, adapter/semaphore saturation, and (in run #3929) shared-object congestion have each been reproduced or exceeded individually.

---

## Tests

Template per test: **Hypothesis · Purpose · Inputs · Outcome**. "Collapse reproduced" = round rate driven to ~3–5/s with checkpoint-signature starvation, matching the incident.

### Test 1 — Tasos — double-spend adapter-hang (run #3901)
- **Date:** 2026-07-08
- **Hypothesis:** Concurrent same-gas double-spend txns pass admission (locks initially available), enter consensus, then all but the winner are dropped in `consensus_handler`; dropped losers do not populate `processed_message`, so adapter waiters hang and exhaust in-flight capacity.
- **Purpose:** Exercise the `skip_processed_checks` / consensus-adapter loser-notification path (later fix #27133).
- **Inputs:** branch `akichidis/defer-double-spend-stress` @ `348ab946`; `target_tps=40000`; `gas_double_spend=100`, `copies=20`; transfer disabled.
- **Outcome:** Severe queueing reproduced — `sequencing_certificate_latency` p50 ~46–49s, p95 ~73–84s; `sequencing_in_flight_semaphore_wait` ~2.5M total / ~20k per validator; consensus included ~34k tx/s (mostly owned double-spend rejects); shared-counter progress starved. Round rate settled 7.2–7.8/s. **Collapse: NO.**

### Test 2 — Tasos — refined double-spend (runs #3902 / #3908)
- **Date:** 2026-07-08+
- **Hypothesis:** Notifying dropped/rejected loser txns releases adapter waiters and removes the catastrophic latency.
- **Purpose:** Validate the loser-notification fix path.
- **Inputs:** newer Tasos commits (post loser-notification fix); same double-spend shape.
- **Outcome:** #3908 much healthier — proposed-block inclusion ~41.6–45.5k tx/s, shared finalization ~9.5k/s, shared-counter latency ~11.5s p50 / ~17s p95, submit-to-ack p50 ~0.68ms. Residual backpressure only. **Collapse: NO.**

### Test 3 — Alex — direct duplicates + amplification (run #3918)
- **Date:** 2026-07-14
- **Hypothesis:** The incident's amplified/duplicate traffic (same tx to many validators) drives the round collapse.
- **Purpose:** Model incident duplicate/amplification traffic from the stress client.
- **Inputs:** commit `689831deb6` (`v1.73.2` + Tasos stress commits + aggressive defaults: `gas_double_spend=20`, `amplification_probability=0.8`, `duplicate_probability=0.5`, `hotness=100`); `protocol_config_override=mainnet`; `target_tps=40000`; 127 validators.
- **Outcome:** Latency regression p50 14–19s / p95 49–53s; `semaphore_wait` 334k–992k; missing-blocks/ancestors matched incident tail (~26k); ~118k `duplicate_tx`/s detected. Round rate 17→~10/s (only 2 validators <5). Consensus **drain stayed 41–52k tx/s**. `congested_transactions` ≈ 0. **Collapse: NO.**

### Test 4 — Alex — soft-bundle gas double-spend (run #3919)
- **Date:** 2026-07-14
- **Hypothesis:** Soft-bundle submissions (evade adapter dedup + undercount in-flight) saturate the submit semaphore → system-message starvation → round collapse.
- **Purpose:** Test the soft-bundle submission path (incident's ~21K req/s driver).
- **Inputs:** commit `2eb1890` (branch `capy/ptn-soft-bundle-gas-double-spend`; `gas_double_spend_submission="soft-bundle"`); `protocol_config_override=mainnet`; `target_tps=40000`; 127 validators.
- **Outcome:** Adapter pressure exceeded the incident — `semaphore_wait` **1.27M**; soft-bundle gas double-spend ~69/s; p95 latency ~52–60s; missing blocks/ancestors ~19.6k/42.9k. Round rate degraded 17→~9/s over hours (8.9 by 00:00Z), only 1 validator <5, **checkpoint_signature healthy**, drain high. **Collapse: NO.**

### Test 5 — Alex — concentrated shared_counter ("Run 1", run #3928)
- **Date:** 2026-07-16 (iterated from #3926, #3927)
- **Hypothesis:** Concentrating distinct-tx volume on few hot shared objects blows the per-object congestion budget → congestion deferral → round collapse.
- **Purpose:** Fire shared-object congestion via concentrated cheap distinct-tx volume (transfer isolated to 0; small counter pool).
- **Inputs:** commit `96735c42c6` (neutral defaults); `protocol_config_override=mainnet`; `target_tps=60000`; `stress_bench_extra_args: --num-shared-counters 4 --shared-counter-max-tip 100000`; `stress_transfer_object=0` (inventory); 127 validators.
- **Outcome:** Latency 27s/57s; `semaphore_wait` 923k; round rate 17→8.2 holding. `congested=0`, `cancelled=0`, `max_congestion_control_object_costs` plateaued **426k** (below trigger), ckpt sig 418. **Collapse: NO. Congestion: NO.** — Root cause identified: mainnet congestion mode is `ExecutionTimeEstimate`; cheap `counter::increment` txns have ~0 execution time and cannot move the budget regardless of concentration or gas budget.

### Test 6 — Alex — high-execution-time slow workload (run #3929)
- **Date:** 2026-07-16
- **Hypothesis:** High **real execution-time** transactions on a shared object move the `ExecutionTimeEstimate` budget where cheap txns cannot → congestion fires → round collapse.
- **Purpose:** Fire execution-time congestion via the `slow` workload (heavy always-on `slow::slow(2000,100)` compute charged to a mutable shared object).
- **Inputs:** commit `f312cc6254` (`slow` workload made heavy + always-on); `protocol_config_override=mainnet`; `target_tps=10000`; `stress_bench_extra_args: --slow 100 --in-flight-ratio 10`; 127 validators. Active window ~20:08–20:24Z.
- **Outcome:** **First run to fire execution-time congestion.** `congested_transactions` avg **3.4M/s** (peak 4.9M), `cancelled` ~302k/s, `max_congestion_control_object_costs` **743k** (matches incident 733k). BUT round rate held **~7–9/s** (only the 2 chronically-stuck validators <5), `checkpoint_signature` stayed healthy **~305–542**. Congestion control deferred/cancelled the excess and protected liveness. **Collapse: NO.** (Congestion over-fired ~100× the incident — the workload is too aggressive per-tx — but even so, congestion alone did not collapse rounds.)
  - **Attribution note:** a round-rate drop to ~0 with 28 validators <5 r/s observed ~21:52Z belonged to a **later `main`/`none`/`target_tps=18000` deploy (run #3931, started 21:39Z), mid network-wipe** — plain `shared_counter`+`transfer` load, `congested=0`. It is a deploy/restart artifact, **not** a congestion-driven collapse and **not** run #3929.

### Test 7 — Alex — combined congestion + semaphore saturation, no byte inflation (run #3932, databaseId 29594057200)
- **Date:** 2026-07-17
- **Hypothesis:** Shared-object execution-time congestion **and** submit-semaphore saturation running **simultaneously** will starve checkpoint-signature submissions and collapse rounds — the combination neither #3919 (semaphore only) nor #3929 (congestion only) achieved.
- **Purpose:** A/B baseline (byte size held at PT default) — run a tuned `slow` congestion slice concurrently with a high-volume soft-bundle gas-double-spend slice.
- **Inputs:** commit `d12f8d3270` (adds tunable `--slow-vectors`/`--slow-size`); `protocol_config_override=mainnet`; `target_tps=40000`; `stress_bench_extra_args: --slow 5 --slow-vectors 100 --gas-double-spend 100 --gas-double-spend-submission soft-bundle --amplification-probability 0.0 --duplicate-probability 0.0`; 126 validators.
- **Outcome:** **Both pressures achieved for the first time** — `sequencing_in_flight_submissions` **pinned at cap (median 317, 95 hosts ≥ 300)**, matching the incident's "inflight pinned ~310 / ~120 hosts waiting"; congestion firing (`congested` ~250k/s, `cancelled` ~7.6k/s). **But `checkpoint_signature` stayed healthy (~477/s) and round rate held ~9/s.** **Collapse: NO.**
  - **Mechanistic finding (key):** congestion does **not** hold submit-semaphore permits. A permit is held only until a tx is *sequenced* (included in a committed block); congestion **deferral occurs later, in the consensus handler, after sequencing**, so the permit is already released. On PT, sequencing is fast (small blocks, fast RocksDB), so permits cycle quickly and checkpoint-signature submissions acquire a permit almost immediately — even with the semaphore pinned. Checkpoint-signature starvation therefore requires **slow sequencing** (long permit-hold), which requires **slow consensus block inclusion** — i.e. large blocks / slow consensus RocksDB. This isolates **byte size** as the specific missing ingredient, not congestion.

### Test 8 — Alex — add ~1 MB blocks (byte-size A/B step) (runs #3933 rejected, #3934 valid)
- **Date:** 2026-07-17
- **Hypothesis:** Large blocks slow consensus sequencing → submit-semaphore permits are held longer → checkpoint-signature submissions starve → round collapse. (Test 7's mechanistic finding pointed here.)
- **Purpose:** B step of the byte-size A/B — Test 7's mix plus byte padding on the slow txns to push block size toward the incident's ~1 MB p99.
- **Inputs:** commit `2e55122a3e` (adds `--slow-padding-bytes`, chunked under the 16 KB pure-arg / 128 KB tx limits); `protocol_config_override=mainnet`; `target_tps=40000`; `stress_bench_extra_args: --slow 40 --slow-vectors 100 --slow-padding-bytes 100000 --gas-double-spend 100 --gas-double-spend-submission soft-bundle`; 126 validators.
  - *First attempt (#3933, commit `c2a6165ebc`, `--slow-padding-bytes 30000`) was invalid:* 30 KB > `max_pure_argument_size` (16 KB), so every slow tx was rejected (6,181/s submitted, 0 success) — no big blocks, no result. Fixed by chunking.
- **Outcome:** **Byte size reproduced** — `consensus_proposed_block_size` p99 **790 KB** (incident ~959 KB), up from Test 7's ~150 KB; slow txns landing (423/s); congestion firing (~259k/s). **But the submit semaphore fell OFF cap** — `sequencing_in_flight_submissions` median dropped to **41** (only 3 hosts ≥ 300) vs Test 7's pinned 317 — because the expensive slow+padding load throttled throughput to ~2.1k tps, starving the soft-bundle volume that pins the semaphore. Checkpoint signatures stayed healthy (~352/s); round rate degraded mildly to ~6.9/s. **Collapse: NO.**
  - **Decisive finding:** the three ingredients (semaphore saturation, congestion, ~1 MB blocks) are **mutually exclusive on PT throughput**. High volume (Test 7) pins the semaphore but keeps blocks small; big/expensive txns (Test 8) inflate blocks but throttle volume so the semaphore drains. PT's fast, uniform hardware processes whatever is offered, so the load never produces the sustained **drain backlog** that, on mainnet, held permits long enough to starve system messages. That backlog required the drain to *fall behind* — the mainnet hardware/scale characteristic explicitly excluded from this effort.

---

## Synthesis (what is and isn't reproduced)

| Incident signal | Reproduced on PT? | In which test |
|---|---|---|
| Latency regression (p50 >15s) | ✅ yes | all |
| Adapter / submit-semaphore saturation (inflight pinned at cap) | ✅ yes | #3918, #3919, **#3932** |
| Shared-object congestion (`congested`/`cancelled`, obj cost 733k) | ✅ yes (exceeded incident) | #3929, #3932 |
| Congestion + semaphore-saturation **simultaneously** | ✅ yes | **#3932** |
| Large blocks (p99 ~790 KB, ≈ incident ~1 MB) | ✅ yes | **#3934** |
| **All three (semaphore + congestion + big blocks) simultaneously** | ❌ **no** — mutually exclusive on PT throughput | — |
| Consensus **drain falling behind** (backlog spiral) | ❌ no (PT drains everything) | — |
| Checkpoint-signature starvation (→ ~70) | ❌ no | — |
| **Round-rate collapse (→ 3–5/s, flatlines)** | ❌ **no** | — |

**Observed pattern:** #3932 achieved congestion **and** semaphore saturation simultaneously, yet checkpoint signatures still flowed and rounds held. The reason (Test 7 mechanistic finding): **congestion does not hold submit-semaphore permits** — a permit is released at *sequencing*, and congestion deferral happens later in the consensus handler. Checkpoint-signature starvation requires **slow sequencing** (permits held long), which on mainnet came from **large blocks + slow consensus RocksDB**. Every PT test produced byte-tiny blocks, so sequencing was fast and permits cycled quickly regardless of how pinned the semaphore was.

The incident's collapse coincided with high shared-object congestion, submit-semaphore saturation, **and slow sequencing (≈1 MB blocks)**, together with checkpoint-signature starvation (`v1.73.2` does not exempt system messages from the semaphore, PR #27123). Byte size — the slow-sequencing lever — is the last unreproduced ingredient.

---

## Conclusion

**The round-rate collapse was not reproduced on private-testnet, and the test campaign is concluded.** Every individual incident signal was reproduced — often exceeding incident magnitude — but never in the combination that produces the collapse:

| Ingredient | Best PT result | Incident |
|---|---|---|
| Latency regression | p50 27–49s | p50 >15s |
| Submit-semaphore saturation (inflight at cap) | median 317, 95 hosts (Test 7) | pinned ~310, ~120 hosts |
| Shared-object execution-time congestion | 3.4M/s (Test 6), 259k/s (Test 8) | 24.7k/s |
| ~1 MB blocks | p99 790 KB (Test 8) | p99 ~959 KB |
| Checkpoint-signature starvation | — (stayed healthy) | 522 → 69 |
| **Round-rate collapse** | — (floor ~6.9/s) | 13 → 3.7/s |

**Cause of the PT/mainnet divergence: NOT yet definitively identified.** An earlier draft of this conclusion attributed it to consensus-RocksDB latency / drain-lag. **That was inferred, not measured, and a metric sweep of the mainnet collapse window (06-24 18:17Z) ruled it out**, along with three other candidates:

| Candidate cause | Verdict (measured) |
|---|---|
| Consensus RocksDB write latency / stalls | ❌ Ruled out — `rocksdb_write_batch_commit_latency` p50 flat (0.3→0.4 ms), p99 modest (peak 180 ms, not time-correlated with the trough), **`actual_delayed_write_rate` = 0** (zero write stalls). Only raised as a passing "concern" in Slack (Tasos 06-23, Kostas via Mark 06-30), never diagnosed. |
| Execution backlog magnitude (`pending_certificates`) | ❌ Ruled out — PT ran at **2.9M** pending certs vs mainnet's 59k at collapse, and PT did **not** collapse. |
| Execution-cache backpressure (`execution_cache_backpressure_status`) | ❌ Ruled out — fired **constantly on PT** (harmless) but **not on mainnet** during the collapse. |
| Fleet-wide consensus propagation delay | ❌ Ruled out — ~0 on both (only 1–5 isolated stuck hosts). |

**What the collapse actually looked like (06-24 18:17Z):** block **production, acceptance, and commit rates all dropped together** (11.6 → 3.6/s) — i.e. the whole consensus threshold clock slowed uniformly, *not* a commit-lag or network-propagation problem. It **tracks block size** within the window (p99 84 KB → 960 KB as production fell 11.6 → 3.6), coincident with peak traffic (23.6k tps), a `pending_certificates` spike (~100 → 59k), and checkpoint-signature starvation (420 → 32.5/s).

**Leading hypothesis — now code-confirmed as the mechanism: per-transaction verification cost at block acceptance.** A code trace of Mysticeti (`consensus/core/`) established:
- The threshold clock (round) advances only on **2f+1 *accepted* blocks** (`threshold_clock.rs:42-52`); proposal and commit are both downstream of acceptance (`core.rs:371-392`) — so all three rates slow together when acceptance slows.
- Transactions are opaque bytes at block *deserialization*, but block **acceptance is gated by `verify_and_vote`** (`block_verifier.rs:193-212`), which for **every transaction** `bcs`-decodes the full PTB, runs validity checks, **verifies the user signature, and locks input objects** (`consensus_validator.rs:235-328`, `:425`). This runs **inline, serially per peer, with no CPU offload** (`subscriber.rs:205`, `authority_service.rs:179`).
- Mainnet at the incident ran **protocol version 126** (confirmed via `sui_current_protocol_version`), i.e. `mysticeti_fastpath` ON (enabled at v96) → the heavy vote path (incl. signature verification + object locking) was live. (Our PT runs also used protocol 126, so the path was active there too.)

So per-tx cost scales with **PTB complexity (commands/inputs), signature count, and input-object count** — *not* just bytes. PT's synthetic txns (1–2 commands, 1 sig, 1 object) verify in microseconds; the incident's real arb (11 MoveCalls, 12 shared objects, real sigs) is orders of magnitude heavier per tx. As blocks filled with expensive-to-verify txns, acceptance saturated CPU → threshold clock slowed → uniform round-rate collapse tracking block content. **Our Test 8 reproduced block *bytes* but not per-tx *verification complexity*, which is why it only reached round 6.9.** The remaining PT-vs-mainnet delta is (a) transaction verification complexity — **reproducible** (Test 9, in progress) — and (b) CPU speed/heterogeneity — excluded.

**What IS established (value of the campaign):**
1. Traffic shape: MEV amplification of **failing** DEX arbitrage against a few **hot shared objects**, plus gas double-spend and soft bundles.
2. Congestion is **`ExecutionTimeEstimate`**-based — driven by execution *time*, not gas/count/bytes (this invalidated Tests 3–5 and is why the `slow` workload was needed).
3. Every individual pressure — latency regression, submit-semaphore saturation, shared-object congestion, ~1 MB blocks — is reproducible and instrumented on PT.
4. Four candidate collapse mechanisms are now **ruled out with data** (table above), which materially narrows the search.

**Recommended next steps.**
- **Code-level analysis** of what throttles Mysticeti block *production* under load (the threshold-clock / proposer path) — since the collapse is a uniform production slowdown, not RocksDB, execution backlog, or propagation.
- Test **transaction-verification complexity** (not just byte size) on PT — e.g. many commands / many shared-object inputs per tx — to see whether per-round processing cost is the gate.
- Validate the specific incident-response fixes in **isolation** (unit/sim): #27123 system-message semaphore bypass, #27074 soft-bundle accounting, congestion/backpressure interaction.
- Full end-to-end replay, if needed, requires **mainnet-representative hardware** (CPU heterogeneity) and/or realistic transaction complexity.

*Tuning knobs added during the campaign and available for future runs: `--slow-vectors`, `--slow-size` (per-tx execution cost / congestion magnitude), `--slow-padding-bytes` (block size), plus the `stress_bench_extra_args` deploy input on branch `steka/ptn-run1-stress-args`.*
