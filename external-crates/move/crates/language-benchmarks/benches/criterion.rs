// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use criterion::{criterion_group, criterion_main, measurement::Measurement, Criterion};
use language_benchmarks::{measurement::cpu_time_measurement, move_vm::bench};

//
// MoveVM benchmarks
//

fn arith<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "arith.move");
}

fn arith_2<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "arith_2.move");
}

fn basic_alloc<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "basic_alloc.move");
}

fn call<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "call.move");
}

fn call_2<M: Measurement + 'static>(c: &mut Criterion<M>) {
    bench(c, "call_2.move");
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
    config = cpu_time_measurement();
    targets =
        arith,
        arith_2,
        basic_alloc,
        call,
        call_2,
        natives,
        transfers,
        vector,
);

criterion_main!(vm_benches);
