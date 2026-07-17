# sui_mainnet_high_traffic — Incident Repro Test Log

Tracking the effort to reproduce the mainnet high-traffic incident's **consensus round-rate collapse** on private-testnet (PT).

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

---

## Synthesis (what is and isn't reproduced)

| Incident signal | Reproduced on PT? | In which test |
|---|---|---|
| Latency regression (p50 >15s) | ✅ yes | all |
| Adapter / submit-semaphore saturation (inflight pinned at cap) | ✅ yes | #3918, #3919, **#3932** |
| Shared-object congestion (`congested`/`cancelled`, obj cost 733k) | ✅ yes (exceeded incident) | #3929, #3932 |
| Congestion + semaphore-saturation **simultaneously** | ✅ yes | **#3932** |
| Large blocks (p99 ~1 MB → consensus RocksDB / propagation load) | ❌ no (all txns byte-tiny) | — |
| Checkpoint-signature starvation (→ ~70) | ❌ no | — |
| **Round-rate collapse (→ 3–5/s, flatlines)** | ❌ **no** | — |

**Observed pattern:** #3932 achieved congestion **and** semaphore saturation simultaneously, yet checkpoint signatures still flowed and rounds held. The reason (Test 7 mechanistic finding): **congestion does not hold submit-semaphore permits** — a permit is released at *sequencing*, and congestion deferral happens later in the consensus handler. Checkpoint-signature starvation requires **slow sequencing** (permits held long), which on mainnet came from **large blocks + slow consensus RocksDB**. Every PT test produced byte-tiny blocks, so sequencing was fast and permits cycled quickly regardless of how pinned the semaphore was.

The incident's collapse coincided with high shared-object congestion, submit-semaphore saturation, **and slow sequencing (≈1 MB blocks)**, together with checkpoint-signature starvation (`v1.73.2` does not exempt system messages from the semaphore, PR #27123). Byte size — the slow-sequencing lever — is the last unreproduced ingredient.

---

## Proposed final test — combine congestion + semaphore saturation to starve system messages

Goal: drive shared-object execution-time congestion **and** submit-semaphore saturation **concurrently**, so that checkpoint-signature / EndOfPublish submissions are starved (no `v1.73.2` bypass) and round progression stalls. Mainnet hardware differences are explicitly excluded.

### Traffic — two simultaneous workload slices
1. **Congestion slice — tuned `slow`.** Dial the per-tx cost **down** from #3929 (which over-fired ~100×) to hit the incident magnitude: `congested_transactions ≈ 24k/s`, `max_congestion_control_object_costs ≈ 733k`. Lower `SLOW_VECTORS`/`SLOW_SIZE` (e.g. `slow(400, 100)`) and/or `--slow` weight so congestion is sustained near the incident level rather than saturating.
2. **Semaphore slice — high-volume soft-bundle gas double-spend.** Run the #3919 soft-bundle path concurrently at high submission rate to pin `sequencing_in_flight_submissions` at cap (~312 fleet-wide) and hold permits for full round-trips. This is the piece that starves system messages.
3. **Byte-size dimension — inflate transaction bytes.** All prior tests (incl. #3929) produced byte-tiny blocks (p99 ~150 KB vs incident's ~1 MB). Add large `vector<u8>` pure-argument padding to the workload transactions so `consensus_proposed_block_size` p99 approaches the incident's ~1 MB. This raises consensus RocksDB write volume and block-propagation bytes — the HW-neutral proxy for the incident's "slower consensus RocksDB under 1 MB blocks," and a direct stressor on the consensus **drain** side that has stayed healthy in every PT test. Requires a small stress-client change (a byte-padding pure arg; there is no existing payload-size knob). Note: byte size does **not** affect congestion (execution-time based) or the semaphore (count based) — its role is purely drain/commit throughput.

Run all slices in the same benchmark mix (separate groups, since amplification and soft-bundle are mutually exclusive per payload), `protocol_config_override=mainnet`.

### Success bar (must co-occur)
- `sequencing_in_flight_submissions` pinned at cap (~312) across most validators, **sustained**.
- `consensus_handler_congested_transactions` ≈ 20–25k/s (incident magnitude, not 100×).
- `consensus_proposed_block_size` p99 approaching ~1 MB (incident level).
- `checkpoint_signature` tx/s falling toward **~70** — the decisive starvation signal.
- → round rate breaking below the ~8–9/s plateau toward 3–5/s.

### Optional accelerator (config, not hardware)
The collapse hinges on the submit semaphore actually pinning. Its cap is `40_000 / num_validators`. To make saturation reliably reachable without touching hardware, lower `max_pending_transactions` in the PT validator overlay (e.g. 20000 → 8000 ⇒ cap ~63 permits at 127 validators), or raise committee size. This shrinks the permit pool so sustained submission volume pins it — matching the incident's "semaphore pinned at cap" fact while staying HW-neutral. Note this diverges from mainnet's exact config; use only if the mainnet-faithful volume cannot pin the semaphore.

### Why this should work where the others didn't
- #3929 proved execution-time congestion is now reproducible (the `slow` workload).
- #3918/#3919 proved the submit adapter can be saturated beyond incident levels.
- Neither starved checkpoint signatures. Combining a congestion backlog (which lengthens consensus round-trips, so each held semaphore permit is held longer) with sustained high-volume soft-bundle submission is the mechanism that pins the semaphore long enough to starve system-message submission — the last missing incident signal before round collapse.
