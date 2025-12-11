// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use criterion::{Criterion, criterion_group, criterion_main, measurement::Measurement};
use language_benchmarks::{
    measurement::wall_time_measurement,
    move_vm::{bench, run_cross_module_tests},
};
use std::path::PathBuf;

//
// MoveVM benchmarks
//

fn arith<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "arith.move");
}

fn basic_alloc<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "basic_alloc.move");
}

fn branch<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "branch.move");
}

fn call<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "call.move");
}

fn loops<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "loop.move");
}

fn natives<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "natives.move");
}

fn transfers<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "transfers.move");
}

fn vector<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "vector.move");
}

fn cross_module<M: Measurement + 'static>(c: &mut Criterion<M>) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = "a1";
    path.extend(["tests", "packages", dir]);
    run_cross_module_tests(c, path);
}

criterion_group!(
    name = vm_benches;
    config = wall_time_measurement();
    targets =
        arith,
        basic_alloc,
        branch,
        call,
        loops,
        natives,
        transfers,
        cross_module,
        // vector,
);

criterion_main!(vm_benches);
