// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module 0x42::m;
fun t0(): u64 {
    let Bla<T>(x) = 0;
    let Self::Bla<T>(x) = 0;
    let ::Bla<T>(x) = 0;
    match (o) {
        Option<u64>::None => 0,
        Self::Option<u64>::Some(_) => 0,
        ::Option<u64>::Other(_) => 0,
    }
}
