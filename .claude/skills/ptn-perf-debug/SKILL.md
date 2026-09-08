---
name: ptn-perf-debug
description: Debug throughput/latency problems on Sui's private testnet (PTN) with an iterative metrics -> theory -> partial deploy -> assess loop, covering validators, fullnodes, and stress nodes.
---

# Debug a private testnet performance issue

Reference files in this directory:
- `metrics.md`: which Prometheus metrics to look at, monitored_scope catalog, cluster-average vs outlier guidance.
- `host-tools.md`: CPU profiling, tcpdump/network analysis, other on-host tools.
- `deploy.md`: PTN topology, observability, host access, sui-operations workflows for full/partial pushes.

## Usage

```
/ptn-perf-debug <description of the symptom, e.g. "TPS dropped from 6k to 3k after Tuesday's deploy">
```

## Ground rules

- Never alter the protocol or protocol config during an experiment. Changes are local optimizations, added
  instrumentation, config knobs, or logging.
- Prefer measuring over guessing. Every deploy costs a build plus a rollout; make sure the theory is falsifiable and
  you know which metric will move before pushing.
- Lock the environment (`environment-lock-unlock.yml`) before a multi-day investigation so the nightly wipe deploy
  does not erase your state. Unlock when done.
- Keep a `PTN_NOTEBOOK.md` in the repo root, one section per iteration:
  ```
  # iteration N
  - OBSERVATIONS: ...
  - HYPOTHESIS: ...
  - EXPERIMENT: <change, hosts it went to, SHA, workflow run URL>
  - RESULTS: confirmed / refuted, and the metrics that showed it
  ```
  Commit notebook updates separately from code (`debug: iteration N observations`), and experiment code separately
  (`debug: iteration N <what>`), so they are easy to drop before a PR.

## Process

### 1. Establish the symptom and the baseline
Ask the user what changed and when (deploy SHA, load change, host changes). Compare the current window against the
last known-good window in Grafana. The continuous regression check config in sui-operations defines "healthy"
(client p50 latency and per-workload TPS thresholds).

### 2. Check the whole stack end to end before going deep
The bottleneck can be anywhere between the stress client and the disk. Walk it in order, at cluster-average level:
1. Stress nodes: are they generating the target load? `num_success` vs target, `no_gas`, `num_in_flight` at cap,
   `num_error{type=rpc}`, host CPU. Read `journalctl -u stress` for errors, retries, lost connections.
2. Fullnode RPC the stress fleet drives: orchestrator finality latency and timeouts, RPC error rates, checkpoint
   execution lag. One overloaded fullnode caps the entire benchmark.
3. Validator ingress: submit latency, overload/load-shedding, transaction driver retries and per-validator errors.
4. Consensus: commit latency, round advancement, proposal starvation, suspended/missing blocks, propagation delay.
5. Consensus handler and execution: handler utilization, execution queue age, deferrals/congestion.
6. Checkpoints: construction/certification/execution ages; who stopped signing.
7. Storage: write stops, compaction debt, batch commit latency.
Then look at outliers for each stage (`topk`, `max by (host)`). A single slow validator often manifests as
cluster-wide consensus latency. If a host is an outlier, decide hardware vs software using host-tools.md.

### 3. Use monitored_scope utilization to find the saturated stage
`rate(monitored_scope_duration_ns[5m]) / 1e9` for the single-threaded scopes (`CoreThread::loop::*`,
`ConsensusCommitHandler::handle_consensus_commit`, `BuildCheckpoints`, `ExecutionDriver::loop`,
`CheckpointExecutor::parallel_step`). Whichever approaches 1.0 is the ceiling. Then break it down with its sub-scopes
(see metrics.md). Remember that `notify_read` scopes and anything spanning an `.await` measure waiting, not CPU.
Check `monitored_tasks{callsite}` / `monitored_futures{callsite}` for the spawn sites in that stage: how many tasks
are live, whether the count is pinned at a limit, leaking, or unexpectedly zero.
After the obvious suspects, explore the code base for other metrics near the suspect stage (grep for
`register_*_with_registry!` in that crate) and for `monitored_mpsc` channel names feeding it.

### 4. Get more detailed measurements when metrics are ambiguous
Per host-tools.md: `perf` CPU profiles on an outlier and a healthy peer, `tcpdump`/`ss`/`nstat` for bandwidth,
retransmits, window sizes, and connection churn, `strace -c`, off-CPU profiles, RocksDB LOG files, config diffs.
Compare against a healthy peer whenever possible; absolute numbers rarely mean much on their own.

### 5. Form a hypothesis and design an experiment
Write the hypothesis and the prediction ("if true, metric X on the updated hosts will drop by Y relative to the
untouched hosts") in the notebook. Check it is consistent with previous iterations. The experiment is a code change
that either attempts a fix or adds instrumentation (new `monitored_scope`s, histograms, `info!("CLAUDE: ...")` logs
that can be grepped in Loki/journalctl). Branch off the deployed SHA so the only difference is your change.

### 6. Push to the minimum number of hosts
Push the branch to `MystenLabs/sui`, then use the sui-operations workflows in deploy.md:
- `sui-node-update.yaml` with `limit=<hosts>` for validators/fullnodes. CPU, storage, and local-path changes: 2 to 4
  hosts (the outlier plus a healthy peer). Network or peer-protocol behavior: both ends must adopt it, so a region or
  the full validator set at `serial<=25%`.
- `stress-node-update.yaml` for stress client changes or load changes.
- `pulumi-sui-deploy.yaml` only when genesis/config generation must change.
Record the SHA, hosts, and run URL in the notebook. Wait for `uptime` to reset on the target hosts before reading.

### 7. Assess
Compare updated hosts vs untouched hosts on the predicted metric over a window long enough to cover normal
variance (at least 15 to 30 minutes at steady load, longer if checkpoints or epochs are involved). Confirmed,
refuted, or inconclusive: record it, and record any side effects (new errors, regressions elsewhere).

### 8. Iterate
Refuted or inconclusive: return to step 3 with the new data. Confirmed but not the root cause: keep going.
Confirmed and the issue is fixed at the target hosts: roll out to the full fleet with `serial=25%` and re-verify
cluster-wide, then run `private-testnet-perf-regression-check.yml`.

### 9. Wrap up
Discuss the fix with the user; the PR must contain only the fix (drop notebook, logging, and experiment commits).
Unlock the environment. Then invoke `/send-pr`.

### 10. Record lessons in this skill
Before finishing, review the notebook for anything that would have shortened the investigation: a metric that was
decisive, a false lead worth warning about, a workflow quirk, a better triage order. Draft the edits to the files in
this directory and show them to the user. Do not commit skill changes without explicit approval; the user may want
to reword or reject them. Keep additions terse.
