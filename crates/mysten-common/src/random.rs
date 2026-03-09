// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::in_antithesis;

/// Get a random number generator.
///
/// If we are running in antithesis mode, use the antithesis RNG.
/// Otherwise, use the rand::thread_rng().
#[inline(always)]
pub fn get_rng() -> impl rand::Rng + rand::CryptoRng {
    if in_antithesis() {
        Either::Antithesis(antithesis_sdk::random::AntithesisRng)
    } else {
        Either::Prod(rand::thread_rng())
    }
}

// Need our own Either implementation so we can impl rand::RngCore for it.
pub enum Either<L, R> {
    Prod(L),
    Antithesis(R),
}

// We don't require that AntithesisRng is a CryptoRng, but it is only
// used in the antithesis test environment.
impl<Prod: rand::CryptoRng, Antithesis> rand::CryptoRng for Either<Prod, Antithesis> {}

impl<L, R> rand::RngCore for Either<L, R>
where
    L: rand::RngCore,
    R: rand::RngCore,
{
    fn next_u32(&mut self) -> u32 {
        match self {
            Either::Prod(l) => l.next_u32(),
            Either::Antithesis(r) => r.next_u32(),
        }
    }

    fn next_u64(&mut self) -> u64 {
        match self {
            Either::Prod(l) => l.next_u64(),
            Either::Antithesis(r) => r.next_u64(),
        }
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        match self {
            Either::Prod(l) => l.fill_bytes(dest),
            Either::Antithesis(r) => r.fill_bytes(dest),
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        match self {
            Either::Prod(l) => l.try_fill_bytes(dest),
            Either::Antithesis(r) => r.try_fill_bytes(dest),
        }
    }
}
