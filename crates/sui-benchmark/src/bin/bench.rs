// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// How-to
// subcommand: `microbench` to run micro benchmarks
// args:
//      running_mode:
//          local-single-validator-thread:
//              start a validator in a different thread.
//          local-single-validator-process:
//              start a validator in a new local process.
//              --working-dir needs to be specified on this mode where a `validator` binary exists

// Examples:
// ./bench microbench local-single-validator-process --port=9555 throughput --working-dir=$YOUR_WORKPLACE/sui/target/release
// ./bench microbench local-single-validator-process latency --working-dir=$YOUR_WORKPLACE/sui/target/release
// ./bench microbench local-single-validator-thread throughput
// ./bench microbench local-single-validator-thread latency

use clap::*;
use std::time::Duration;
use sui_benchmark::benchmark::{
    bench_types, run_benchmark, validator_preparer::VALIDATOR_BINARY_NAME,
};
use tracing::subscriber::set_global_default;
use tracing_subscriber::EnvFilter;

// For memory profiling info see https://github.com/jemalloc/jemalloc/wiki/Use-Case%3A-Heap-Profiling
// Example: set JE_MALLOC_CONF or _RJEM_MALLOC_CONF to:
//   prof:true,lg_prof_interval:24,lg_prof_sample:19
// The above means: turn on profiling, sample every 2^19 or 512KB bytes allocated,
//   and dump out profile every 2^24 or 16MB of memory allocated.
//
// See [doc/src/contribute/observability.md] for more info.
#[cfg(not(target_env = "msvc"))]
use jemalloc_ctl::{epoch, stats};
#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    #[cfg(not(target_env = "msvc"))]
    malloc_conf();

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");
    let benchmark = bench_types::Benchmark::parse();
    running_mode_pre_check(&benchmark);

    #[cfg(not(target_env = "msvc"))]
    std::thread::spawn(|| {
        loop {
            // many statistics are cached and only updated when the epoch is advanced.
            epoch::advance().unwrap();

            let allocated = stats::allocated::read().unwrap() / (1024 * 1024);
            let resident = stats::resident::read().unwrap() / (1024 * 1024);
            println!(
                "Jemalloc: {} MB allocated / {} MB resident",
                allocated, resident
            );
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    let r = run_benchmark(benchmark);
    println!("{}", r);
}

#[cfg(not(target_env = "msvc"))]
fn malloc_conf() {
    use jemalloc_ctl::config;
    let malloc_conf = config::malloc_conf::mib().unwrap();
    println!("Default Jemalloc conf: {}", malloc_conf.read().unwrap());
}

fn running_mode_pre_check(benchmark: &bench_types::Benchmark) {
    match benchmark.running_mode {
        bench_types::RunningMode::SingleValidatorThread => {}
        bench_types::RunningMode::SingleValidatorProcess => match &benchmark.working_dir {
            Some(path) => {
                assert!(
                    path.clone().join(VALIDATOR_BINARY_NAME).is_file(),
                    "validator binary needs to be in working-dir"
                );
            }
            None => panic!("working-dir option is required in local-single-authority-process mode"),
        },
        bench_types::RunningMode::RemoteValidator => {
            unimplemented!("Remote benchmarks not supported through this entrypoint")
        }
    }
}
