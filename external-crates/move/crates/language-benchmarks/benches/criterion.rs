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

fn const_u64<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_u64_constants.move"); }
fn const_many_u64<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_many_u64_constants.move"); }
fn const_address<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_address_constants.move"); }
fn const_bool<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_bool_constants.move"); }
fn const_vec_small<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_vector_constant.move"); }
fn const_vec_256<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_vec_256.move"); }
fn const_vec_1k<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_vec_1k.move"); }
fn const_vec_u64<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_vec_u64_128.move"); }
fn const_vec_addr<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_vec_addr_32.move"); }
fn const_vec_256_acc<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_vec_256_accumulate.move"); }
fn const_vec_1k_acc<M: Measurement + 'static>(c: &mut Criterion<M>) { bench(c, "const_bench_load_vec_1k_accumulate.move"); }

criterion_group!(
    name = vm_benches;
    config = wall_time_measurement();
    targets =
        arith,
        arith_2,
        basic_alloc,
        call,
        call_2,
        natives,
        transfers,
        vector,
        const_u64,
        const_many_u64,
        const_address,
        const_bool,
        const_vec_small,
        const_vec_256,
        const_vec_1k,
        const_vec_u64,
        const_vec_addr,
        const_vec_256_acc,
        const_vec_1k_acc,
);

criterion_main!(vm_benches);
