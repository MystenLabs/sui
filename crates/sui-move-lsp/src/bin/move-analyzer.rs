// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use move_analyzer::analyzer;
use move_compiler::editions::Flavor;
use sui_package_alt::SuiFlavor;

#[cfg(target_os = "linux")]
mod alloc_utils {
    // On Linux, jemaclloc produces better results in terms of memory usage.
    // Standard allocator does not work too well for cases when a lot of memory
    // is allocated temporarily and then freed as tends to hold on to allocated
    // memory rather than returning it to the OS right away.
    use tikv_jemallocator::Jemalloc;
    #[global_allocator]
    static GLOBAL: Jemalloc = Jemalloc;

    pub fn maybe_enable_jemalloc() {
        match tikv_jemalloc_ctl::version::read() {
            Ok(version) => eprintln!("jemalloc version = {}", version),
            Err(error) => eprintln!("cannot read jemalloc version: {}", error),
        }
        // enable background purge threads
        let _ = tikv_jemalloc_ctl::background_thread::write(true);
        let _ = tikv_jemalloc_ctl::epoch::advance();
    }
}

#[cfg(not(target_os = "linux"))]
mod alloc_utils {
    // We could use a jemalloc here as well but standard allocator
    // on MacOS is better tuned even for the specific workload
    // we are dealing with, and jemalloc on Windows is not well supported
    // so we are not going to use it there either, particularly
    // that the standard allocator on Windows is known to be well tuned as well.

    pub fn maybe_enable_jemalloc() {
        eprintln!("using standard allocator");
    }
}

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[derive(Parser)]
#[clap(
    name = env!("CARGO_BIN_NAME"),
    rename_all = "kebab-case",
    author,
    version = VERSION,
)]
struct App {}

fn main() {
    App::parse();
    alloc_utils::maybe_enable_jemalloc();
    analyzer::run::<SuiFlavor>(Some(Flavor::Sui));
}
