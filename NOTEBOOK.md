# Iteration 1

- OBSERVATIONS
  - PR 27959 changes only the ordinary GraphQL checkpoint-resume integration test. It does not change the benchmark or surfer code.
  - Its mainnet simulator CI job 102960697534 failed `test::test_simulated_load_shared_object_congestion_control` on all four attempts at seed 1187333251852574750.
  - The assertion at `crates/sui-benchmark/tests/simtest.rs:1554` concerns the auxiliary surfer, not the benchmark driver's transaction count: `results.num_successful_transactions > 0`.
  - The failure occurred after 71 seconds on the last attempt, not at the five-minute test deadline.
- HYPOTHESIS
  - The seed exposes a baseline surfer progress/coverage failure under congestion, independent of the GraphQL test change. Existing info logs can distinguish no executed calls from calls whose effects failed.
- EXPERIMENT
  - Run the unchanged main base 5fd7552ce9b82f08db8fc320d19e617b8fa1ee37 with the CI seed and mainnet override, using the simulator wrapper, the exact test filter, no package selector, no retries, and captured info/debug logs.
  - Preserve functional code and simulator pins. Do not alter a PR revision to select another seed.
- RESULTS
  - The unchanged baseline run passed in 60.088 seconds. Its retained output does not contain the surfer publication status or final statistics, so it does not explain the Linux CI failure.
  - The GraphQL cursor change remains accepted and unchanged. No causal conclusion follows from this one baseline pass.

# Iteration 2

- OBSERVATIONS
  - `SurferTask::create_surfer_tasks` excludes the benchmark accounts before constructing its account map.
  - Package publication uses the surfer's own address and gas object. `process_tx_effects` updates the reserved gas reference from transaction effects.
  - `publish_package` treats a successful RPC response as publication success without checking execution status. Failed effects could leave the callable-function registry empty, but the retained CI output does not establish that this happened.
- RESULTS
  - Account selection and stale reserved gas references are not supported explanations from this inspection.
  - Publication status and surfer progress remain unresolved. Do not increase deadlines, change gas pricing, or claim a cursor-related cause without evidence.

# Iteration 3

- OBSERVATIONS
  - The unchanged PR's mainnet simulator rerun, job 103093415527, failed the same surfer progress assertion on all four attempts at seed 1187333251852574750. The suite finished with 3140 passed and one failed.
  - CI sets `SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE=mainnet`. The earlier local receipt records intended chain and seed, but not the complete command/environment.
  - CI builds the workspace without a package or test-target selector. The earlier local run selected a test target.
- HYPOTHESIS
  - Chain configuration or workspace feature unification may account for the missing local reproduction; platform differences remain another possibility.
- EXPERIMENT
  - Use the exact chain-override variable and seed, the CI nextest profile, no package or test-target selector, and an exact nextest test filter. Enable existing info logs and disable retries; make no functional changes.
- RESULTS
  - Passed on macOS in 60.239 seconds, with the CI profile and all 330 workspace test binaries built. No info-level telemetry appeared, so the progress state remains unobserved.
  - Source inspection found no telemetry subscriber initialization in this integration test. The earlier local pass is not evidence that the Linux failure is fixed.

# Iteration 4

- OBSERVATIONS
  - CI checked out merge 107a414d42aea01e20864efd85e4792f39edea83, combining the accepted cursor change with main 5fd7552ce9b82f08db8fc320d19e617b8fa1ee37.
  - No Linux SSH host is configured. The existing Rust workflow supports dispatch on an isolated task branch.
- HYPOTHESIS
  - Failed package execution may leave the surfer with no callable functions; publication currently reports RPC success without checking execution status.
- EXPERIMENT
  - Use a separate diagnostic branch based on that exact CI merge. Initialize test logging and log package execution status; retain all original functional code and assertions.
  - Dispatch only the existing Linux mainnet simulator job, with the original seed and watchdog deadline, a single exact nextest filter, no retries, and scoped info logs. Disable unrelated root jobs only on this unmerged diagnostic branch; do not change credentials, cache policy, or any replacement PR.
- RESULTS
  - Linux run 34548064848 passed the selected test in 60.106 seconds. Both packages executed successfully; the surfer completed 17 successful and three failed transactions.
  - The observed directory order was `move_building_blocks`, then `random`. The initial surfer seed was 13470328078362337456; congestion target utilization was 5 and maximum deferrals 694.
  - This is not a repair or proof that the original CI failure disappeared. Instrumentation and selected-test execution differ from the failing full-cohort run.

# Iteration 5

- HYPOTHESIS
  - Unsorted filesystem enumeration changes package publication and callable-function order, changing seeded exploration and progress under congestion.
- EXPERIMENT
  - Keep iteration 4's source, seed, filtering, logging, and deadlines unchanged. On Linux, reinsert the two existing package directories in reverse order without changing their contents.
  - Assert the actual `os.scandir` order and verify that git reports no package-content difference before running the same simulator command.
- RESULTS
  - Pending.
