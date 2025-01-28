// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module a::b;

fun lambda_tys_complex() {
   a::b::macro_call!(
       |b1: &mut a::c::C<X,Y>,
        b2: &mut LocalTy,
        b3: u128| say::hello()
   )
}

fun lambda_tys_simple() {
    core::create_ticket!(
        |pool: u64, delta_l: u128| say::hello()
    )
}

fun lambda_no_tys() {
    core::create_ticket!(
        |pool, delta_l| say::hello()
    )
}
