// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::crypto::sha3_hash;
use curve25519_dalek::ristretto::RistrettoPoint;

#[allow(clippy::wrong_self_convention)]
pub trait IntoPoint {
    fn into_point(&self) -> RistrettoPoint;
}

impl<T> IntoPoint for &T
where
    T: IntoPoint,
{
    fn into_point(&self) -> RistrettoPoint {
        (*self).into_point()
    }
}

/*
   A MulHash accumulator: each element is mapped to a
   point on an elliptic curve on which the DL problem is
   hard. The accumulator is the sum of all points.

    See for more information about the construction and
    its security: https://arxiv.org/abs/1601.06502

*/
#[derive(Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accumulator {
    accumulator: RistrettoPoint,
}

impl Accumulator {
    /// Insert one item in the accumulator
    pub fn insert<I>(&mut self, item: &I)
    where
        I: IntoPoint,
    {
        let point: RistrettoPoint = item.into_point();
        self.accumulator += point;
    }

    // Insert all items from an iterator into the accumulator
    pub fn insert_all<'a, I, It>(&'a mut self, items: It)
    where
        It: 'a + IntoIterator<Item = &'a I>,
        I: 'a + IntoPoint,
    {
        for i in items {
            self.insert(i);
        }
    }

    /// Remove one item from the accumulator
    pub fn remove<I>(&mut self, item: &I)
    where
        I: IntoPoint,
    {
        let point: RistrettoPoint = item.into_point();
        self.accumulator -= point;
    }

    // Remove all items from an iterator from the accumulator
    pub fn remove_all<'a, I, It>(&'a mut self, items: It)
    where
        It: 'a + IntoIterator<Item = &'a I>,
        I: 'a + IntoPoint,
    {
        for i in items {
            self.remove(i);
        }
    }

    pub fn digest(&self) -> [u8; 32] {
        sha3_hash(self)
    }
}

impl Debug for Accumulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Accumulator").finish()
    }
}

#[cfg(test)]
mod tests {
    use rand::seq::SliceRandom;

    use crate::accumulator::Accumulator;
    use crate::base_types::ObjectDigest;

    #[test]
    fn test_accumulator() {
        let ref1 = ObjectDigest::random();
        let ref2 = ObjectDigest::random();
        let ref3 = ObjectDigest::random();
        let ref4 = ObjectDigest::random();

        let mut a1 = Accumulator::default();
        a1.insert(&ref1);
        a1.insert(&ref2);
        a1.insert(&ref3);

        // Insertion out of order should arrive at the same result.
        let mut a2 = Accumulator::default();
        a2.insert(&ref3);
        assert_ne!(a1, a2);
        a2.insert(&ref2);
        assert_ne!(a1, a2);
        a2.insert(&ref1);
        assert_eq!(a1, a2);

        // Accumulator is not a set, and inserting the same element twice will change the result.
        a2.insert(&ref3);
        assert_ne!(a1, a2);
        a2.remove(&ref3);

        a2.insert(&ref4);
        assert_ne!(a1, a2);

        // Supports removal.
        a2.remove(&ref4);
        assert_eq!(a1, a2);

        // Removing elements out of order should arrive at the same result.
        a2.remove(&ref3);
        a2.remove(&ref1);

        a1.remove(&ref1);
        a1.remove(&ref3);

        assert_eq!(a1, a2);

        // After removing all elements, it should be the same as an empty one.
        a1.remove(&ref2);
        assert_eq!(a1, Accumulator::default());
    }

    #[test]
    fn test_accumulator_insert_stress() {
        let mut refs: Vec<_> = (0..100).map(|_| ObjectDigest::random()).collect();
        let mut accumulator = Accumulator::default();
        accumulator.insert_all(&refs);
        let mut rng = rand::thread_rng();
        (0..10).for_each(|_| {
            refs.shuffle(&mut rng);
            let mut a = Accumulator::default();
            a.insert_all(&refs);
            assert_eq!(accumulator, a);
        })
    }

    #[test]
    fn test_accumulator_remove_stress() {
        let mut refs1: Vec<_> = (0..100).map(|_| ObjectDigest::random()).collect();
        let mut refs2: Vec<_> = (0..100).map(|_| ObjectDigest::random()).collect();
        let mut accumulator = Accumulator::default();
        accumulator.insert_all(&refs1);

        let mut rng = rand::thread_rng();
        (0..10).for_each(|_| {
            refs1.shuffle(&mut rng);
            let mut a = Accumulator::default();
            a.insert_all(&refs1);
            a.insert_all(&refs2);
            refs2.shuffle(&mut rng);
            a.remove_all(&refs2);
            assert_eq!(accumulator, a);
        })
    }
}
