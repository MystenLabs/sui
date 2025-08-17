// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use criterion::{Criterion, criterion_group, criterion_main, measurement::Measurement};
use language_benchmarks::{measurement::wall_time_measurement, move_vm::bench};

//
// MoveVM benchmarks
//

fn arith<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "arith.move");
}

fn basic_alloc<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "basic_alloc.move");
}

fn call<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "call.move");
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

criterion_group!(
    name = vm_benches;
    config = wall_time_measurement();
    targets =
        arith,
        basic_alloc,
        call,
        natives,
        transfers,
        vector,
);

criterion_main!(vm_benches);
