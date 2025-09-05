// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::seq::SliceRandom;
use rand::Rng;
use sui_macros::nondeterministic;

use crate::in_test_configuration;
use crate::random::get_rng;

pub fn randomize_cache_capacity_in_tests<T>(size: T) -> T
where
    T: Copy + PartialOrd + rand::distributions::uniform::SampleUniform + TryFrom<usize>,
{
    if !in_test_configuration() {
        return size;
    }

    let mut rng = get_rng();

    // Three choices for cache size
    //
    // 2: constant evictions
    // size: presumably chosen to minimize evictions
    // random: thrown in case there are weird behaviors at specific but arbitrary eviction/miss rates.
    //
    // We don't simply use a uniform random choice because the most interesting cases are probably
    // a) the value that will be used in production and b) a very tiny value so we want those two
    // cases to get picked more often.

    // using unwrap() invokes all sorts of requirements on Debug impls.
    let Ok(two) = T::try_from(2) else {
        panic!("Failed to convert 2 to T");
    };

    let random_size = rng.gen_range(two..size);
    let choices = [two, size, random_size];
    *choices.choose(&mut rng).unwrap()
}

pub type TempDir = tempfile::TempDir;

/// Creates a temporary directory with random name.
/// Ensure the name is randomized even in simtests.
pub fn tempdir() -> std::io::Result<TempDir> {
    nondeterministic!(tempfile::tempdir())
}
