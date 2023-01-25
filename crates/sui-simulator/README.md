
# Sui Simulation Testing

This document outlines what the simulator used by `cargo simtest` enables, how it works, how to write sim tests,
and outlines some future work.

## What its for:

Currently, the simulator:

- Provides deterministic, randomized execution of an entire Sui network in a single process.
- Simulates network latency and packet loss as desired.

This allows us to:

- Run tests under adverse network conditions, including high latency, packet loss, and total partitions.
- Run many iterations of tests with different starting seeds, to attempt to expose rare bugs.
- Reproduce bugs easily, once found, by re-running the test with a given seed.

## How it works:

The code for the simulator itself lives in the https://github.com/MystenLabs/mysten-sim repository.
It has the following main components:

1. A [runtime](https://github.com/MystenLabs/mysten-sim/blob/main/msim/src/sim/runtime/mod.rs) which provides:
    - A "node" context for all running tasks. The node is a simulated machine, which can be killed, restarted, or paused.
    - A randomized but deterministic executor.
    - Simulated clock facilities, including timers, sleep(), etc.
    - A global, seeded PRNG used to provide all random behavior throughout the simulator.

1. A network simulator, which delivers network messages between nodes, and can inject latency and packet loss.

1. An API-compatible replacement for tokio.
    - Most facilities from `tokio::runtime` and `tokio::time` are delegated back to the simulator runtime.
    - Custom implementations of the `tokio::net::Tcp*` structs are provided to interface with the network simulator.
    - Most other pieces of tokio (e.g. `sync`) did not need to be re-implemented because they don't interface with the runtime or the network. These are simply re-exported as is.
    - A minimal [fork of tokio](https://github.com/mystenmark/tokio-madsim-fork) is required in order to expose certain internals to the simulator. This fork has very few modifications, which were written to be easily rebaseable when new tokio releases come out.

1. A library of interceptor functions which intercept various posix API calls in order to enforce determinism throughout the test. These include:
    - `getrandom()`, `getentropy()` - intercepted and delegated to the simulator PRNG.
    - Various [socket api calls](https://github.com/MystenLabs/mysten-sim/blob/main/msim/src/sim/net/mod.rs#L195), which intercept networking operations and route them through the network simulator. It was necessary to do this at a very low level because [Quinn](https://github.com/quinn-rs/quinn) does its UDP I/O via direct `libc::` calls rather than using `tokio::net::UdpSocket`.
    - `mach_absolute_time()`, `clock_gettime()`: Intercepted to provide deterministic high-resolution timing behavior.
    - TODO: `gettimeofday()`: We would like to intercept this to provide deterministic wall-clock operations (e.g. on dates, etc). However, intercepting this currently breaks RocksDB.

    This interception behavior is in effect only in threads that have explicitly enabled it, which generally includes the main test thread only. In other threads, the interceptors delegate the call to the system library implementation via `dlsym()`. See implementation [here](https://github.com/MystenLabs/mysten-sim/blob/main/msim/src/sim/intercept.rs#L34-L48).

1. Procedural macros that replace `#[tokio::test]` and run test code inside a testing environment. These are `#[sui_test]` and `#[sim_test]` and are documented below. The test harness created by these macros initializes the simulator runtime with a starting seed, generates the simulator configuration, and runs the test inside a newly created thread. The test must be run in its own thread in order to provide each test case with fresh thread local storage.


## How to run sim tests

First, you'll have to install the `simtest` sub-command by running:

     $ ./scripts/simtest/install.sh

You can then run tests by doing:

     $ cargo simtest

The simtest command calls `cargo nextest`, so you can add any valid `nextest` option to the command line.

`cargo simtest` also reads the following environment variables:

- `MSIM_TEST_SEED` - the random seed for the global PRNG. Must be a positive decimal integer that fits into a `u64`. The default value is `1`.

- `MSIM_TEST_NUM` - the number of times to repeat each test. Each repetition of a test is done with a different random seed, starting from the value of `MSIM_TEST_SEED` for the first repetition. The next seed is computed using the following function:

        fn next_seed(seed: u64) -> u64 {
            use rand::Rng;
            rand::GlobalRng::new_with_seed(seed).gen::<u64>()
        }

    This means that if you run these two commands:

        $ MSIM_TEST_SEED=1 MSIM_TEST_NUM=10 cargo simtest
        $ MSIM_TEST_SEED=2 MSIM_TEST_NUM=10 cargo simtest

    No two iterations will have the same seed (with very high probability).

- `MSIM_TEST_CHECK_DETERMINISM` - if set, the specified tests will be run twice, and the framework will verify that the test executes identically in both runs. (This check can also be done by defining a test case with: `#[sim_test(check_determinism)]`.). *Note: Many existing tests in sui do not pass this check, which runs the test case twice in the same process, although those same tests do execute identically if run twice in separate processes. This is a bug, is most likely due to tests sharing static storage or on-disk state, and will hopefully be fixed shortly.*


## How to write simulation tests:

Simulation tests are declared in one of the following two ways:

      using sui_macros::*;

      #[sui_test]
      async fn test1() {
          // A test that will run using `#[tokio::test]` when run via `cargo nextest`, or
          // else a simulator test when run via `cargo simtest`.
      }

      #[sim_test]
      async fn test2() {
          // A test that will be ignored when run via `cargo nextest`, and only run
          // via `cargo simtest`.
      }

The `#[sim_test]` proc macro also takes a number of arguments, described below.

The easiest way to write tests that run in the simulation testing framework is to use [SwarmBuilder](https://github.com/MystenLabs/sui/blob/main/crates/sui-swarm/src/memory/swarm.rs#L47) to start your validators.
This is most often called indirectly via `start_test_network` in the test-utils crate.
Swarm will create one simulator node (i.e. a simulated machine) per validator, and each validator will have its own unique IP address.

If you use Swarm, you usually will not have to write any code that is aware of the fact that it is running in the simulator.
However, the fact that the validators are running on unique simulator nodes means you will be able to add network latency, packet loss, and partitions to your test later on.

### `SuiNodeHandle`

Swarm assumes a level of encapsulation that reflects what client code would actually experience in production.
In other words, the only way to communicate with the validators when using Swarm is via the network.
However, we have many tests that create validators and manipulate them more directly.
https://github.com/MystenLabs/sui/blob/main/crates/sui/tests/checkpoints_tests.rs is a good example of this.

In these tests, the test code is able to break the simulator abstraction and directly manipulate the state of remote validators.
Yet, the validators are still running on simulated nodes.
This can cause problems if the test code spawns a task on behalf of a remote validator.
The spawned task will appear to the simulator to be executing inside the client node rather than the validator node.
If that spawned task initiates a network connection, it will appear to originate from the client node rather than the validator node.

To address this, most such test code launches validators via the `spawn_test_authorities` function in the `test-utils` carate, which returns `Vec<SuiNodeHandle>` rather than `Vec<SuiNode>`.
`SuiNodeHandle` hides the `SuiNode` from the test code.
It can only be accessed as follows:

        handle.with(|node| {
            let state = node.state();
            do_stuff_to_state(state);
        });

Or in the case of async code:

        handle.with_async(|node| async move {
            let state = node.state();
            do_async_stuff_to_state(state).await;
        }).await;

(`with_mut` and `with_mut_async` are also available).

`SuiNodeHandle` runs the provided callbacks/futures inside the context of the appropriate simulator node, so that network requests, spawned tasks, etc continue running in the correct context.

Note that it is trivial to exfiltrate state from the remote node, e.g.:

        let node_state = handle.with(|node| {
            node.state()
        });

        // Never do this!
        spawn_task_on_state_in_client_node(node_state);

It's not feasible to completely prevent this from happening - the API is just designed to make the correct thing as easy as possible.

Also, the world will not end if you break this rule. You just might see confusing behavior in your tests.

### The #[sim_test] macro

`#[sim_test]` currently accepts two arguments:

- `config = "config_expr"` - This argument accepts a string which will be evaluated as an expression that returns the configuration for the test. Generally, you should make this a function call, and then define the function to return the config. The function must return a type that can implements `Into<TestConfig>` - the most common choice is `SimConfig`, but `Vec<SimConfig>` and `Vec<(usize /* repeat count */, SimConfig)>` are also supported by default. See https://github.com/MystenLabs/mysten-sim/blob/main/msim/src/sim/config.rs for the `TestConfig` implementation.

- `check_determinism` - If set, the framework will run the test twice, and verify that it executes identically each time. (It does this by keeping a log which contains an entry for every call to the PRNG. Each entry contains a hash of the value yielded by the PRNG at that point + the current time.). Tests with `check_determinism` are usually for testing the framework itself, so you probably won't need to use this.

### Configuring the network:

Network latency and packet loss can be configured using [SimConfig](https://github.com/MystenLabs/mysten-sim/blob/main/msim/src/sim/config.rs#L8), which is re-exported from this crate as `sui_simulator::config::SimConfig`.

To configure a test, you write:

      fn my_config() -> SimConfig {
         ...
      }

      #[sim_test(config = "my_config()")]
      async fn test_case() {
         ...
      }

A vector a SimConfigs can also be returned, in order to run the same test under multiple configurations.
For instance, you might do:

     fn test_scenarios -> Vec<SimConfig> {
        vec![
          fast_network_config(), // low latency
          slow_network_config(), // high latency
          lossy_network_config(), // packet loss
        ]
     }

      #[sim_test(config = "test_scenarios()")]
      async fn test_case() {
         ...
      }

Documentation of network configuration is not finished yet, but reading the code for the [NetworkConfig](https://github.com/MystenLabs/mysten-sim/blob/main/msim/src/sim/net/config.rs#L221) should be very instructive.

There is a small but growing library of functions for building network configs in [sui_simulator::configs](https://github.com/MystenLabs/sui/blob/main/crates/sui-simulator/src/lib.rs).

There are also some examples of network configuration at https://github.com/MystenLabs/sui/blob/main/crates/sui-benchmark/tests/simtest.rs#L52.

### The `nondeterministic!` macro

Occasionally a test needs an escape hatch from its deterministic environment.
The most common such case is when creating a temporary directory.
For these situations, the `nondeterministic!` macro is provided.
It can be used to evaluate any expression in another thread, in which the system interceptor functions (e.g. `getrandom()`) are disabled.
For example:

        let path = dir.join(format!("DB_{:?}", nondeterministic!(ObjectID::random())));

Without the `nondeterministic!` macro, this code could generate the same path in two different tests (each test is started with the same seed).

## Flaky tests

One of the benefits of deterministic simulation is that it eliminates flaky tests.
However, that is a half truth: The simulator framework tries to guarantee that a given test binary will produce the exact same behavior when run twice with the same seed.
This means that in general, tests shouldn't be flaky in the usual sense.
If they fail once, retrying them won't help.
Similarly, if they pass once (at a given commit), they should never fail at that commit.

However, tests may be "intrinsically flaky" - for instance they may be dependent on precise timing.
As a reductio, you could imagine a test that simply samples a random number at some point, and fails if is greater than a threshold.
This test will either fail or pass repeatably when run with the same seed at the same commit.
However, that test is obviously flaky by its nature.

Further, the simulator framework is unavoidably very susceptible to butterfly effects - almost any event (e.g. spawning a new task, sending data over the network, sampling a random number) will cause every event that comes later to play out differently.
Network messages will have different delays, tasks will execute in a different order, random samples will change.

This means that almost any code change could cause a totally unrelated flaky test to suddenly start failing due to butterfly effects.
In such cases, the blame goes to the test that started failing - it was clearly passing by chance only.
The good news is that because the system is deterministic, the flaky test can usually be debugged quickly.

### How to fix a flaky test

If you find a flaky test, here are some tips on how to start.
First, ascertain that the test fails repeatedly when run in isolation, e.g.:

      $ cargo simtest my_flaky_test

If it fails when run *en masse*, but passes when run individually (or vice versa) then there may be a test isolation failure.
Test isolation failures are often due to filesystem or static memory access, or may be simulator bugs.

Assuming that the test does fail repeatably in isolation, you can try the test with several different seeds:


      $ MSIM_TEST_SEED=XXX cargo simtest my_flaky_test

And see how often if passes/fails.
Note which seeds cause the test to pass, and which cause it to fail - it may be helpful to compare log output from passing and failing runs to see how they differ, and between pairs of passing or failing runs to see how they are alike.

**Do not forget that the test may not be to blame!**
Your code may be just be buggy!
In other words, flaky code may also cause flaky tests.

It's impossible to list every possible cause of flakiness in a document, but the best place to start looking is at anything timing related, especially hard-coded delays in the test.
n t
Once you have found the bug or the source of the flakiness, you can validator your fix by running:

      $ MSIM_TEST_NUM=20 cargo simtest my_flaky_test

This will run your test 20 times with different seeds.
Feel free to increase this number - theoretically working code should work no matter how many times you repeat the test, but you probably don't have time for more than a few dozen iterations.

(Soon, we will add a nightly workflow that runs all tests with a high iteration count in order to pro-actively find bugs and flaky tests.)

## Reproducing failures

The point of having a deterministic execution environment for tests is that failures can be reproduced easily once found.

When tests fail, they print out a random seed that you can use to reproduce the failure.
In normal CI runs, this seed should always be `1` (the default value).
However, when a test is run with a higher iteration count, the reported seed will be some large random number.
Ideally you should be able to immediately reproduce the failure by running a single iteration of the test with the given seed.

*Currently, this feature is in progress due to some isolation failures in the simulator framework that I am trying to track down.*

**Linux vs Mac**: There is one big caveat here, which is that we don't have identical execution across different platforms. This may be impossible to achieve due to `#[cfg(target_os = xxx)]` attributes in our dependencies. Therefore, even once test re-execution is fully supported, it may be necessary to use linux (perhaps via a local docker container) to reproduce failures found on our CI machines.
