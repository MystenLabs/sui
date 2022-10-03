// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "celo")]
pub struct RngWrapper<'a, R: rand::CryptoRng + rand::RngCore>(pub &'a mut R);

impl<R: rand::CryptoRng + rand::RngCore> ark_std::rand::RngCore for RngWrapper<'_, R> {
    #[inline]
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    #[inline]
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }

    // The feature disjunction is because celo's bls-crypto fails to activate ark-std/std
    #[cfg(feature = "std")]
    #[inline]
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), ark_std::rand::Error> {
        #[cfg(feature = "celo")]
        self.0
            .try_fill_bytes(dest)
            .map_err(|e| ark_std::rand::Error::new(e.take_inner()))
    }

    #[cfg(not(feature = "std"))]
    #[inline]
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), ark_std::rand::Error> {
        self.0
            .try_fill_bytes(dest)
            .map_err(|_| ark_std::rand::Error::from(std::num::NonZeroU32::new(1).unwrap()))
    }
}
