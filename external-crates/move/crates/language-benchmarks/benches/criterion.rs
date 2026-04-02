// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use criterion::{Criterion, criterion_group, criterion_main, measurement::Measurement};
use language_benchmarks::measurement::wall_time_measurement;
use language_benchmarks::move_vm::bench;

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

fn structs<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "structs.move");
}

fn references<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "references.move");
}

fn generics<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "generics.move");
}

fn large_functions<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "large_functions.move");
}

fn enums<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "enums.move");
}

/// Interpreter step() overhead benchmarks.
/// These measure raw dispatch overhead with minimal work per instruction.
/// Use to validate tracing optimizations:
/// - Without tracing: `cargo bench -p language-benchmarks -- interpreter_step`
/// - With tracing: `cargo bench -p language-benchmarks --features move-vm-runtime/tracing -- interpreter_step`
fn interpreter_step<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "interpreter_step.move");
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
        vector,
        structs,
        references,
        generics,
        large_functions,
        enums,
        interpreter_step,
);

criterion_main!(vm_benches);
