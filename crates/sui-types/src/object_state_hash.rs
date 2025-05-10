// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub type ObjectStateHash = fastcrypto::hash::EllipticCurveMultisetHash;

#[cfg(test)]
mod tests {
    use crate::base_types::ObjectDigest;
    use crate::object_state_hash::ObjectStateHash;
    use fastcrypto::hash::MultisetHash;
    use rand::seq::SliceRandom;

    #[test]
    fn test_object_state_hash() {
        let ref1 = ObjectDigest::random();
        let ref2 = ObjectDigest::random();
        let ref3 = ObjectDigest::random();
        let ref4 = ObjectDigest::random();

        let mut a1 = ObjectStateHash::default();
        a1.insert(ref1);
        a1.insert(ref2);
        a1.insert(ref3);

        // Insertion out of order should arrive at the same result.
        let mut a2 = ObjectStateHash::default();
        a2.insert(ref3);
        assert_ne!(a1, a2);
        a2.insert(ref2);
        assert_ne!(a1, a2);
        a2.insert(ref1);
        assert_eq!(a1, a2);

        // ObjectStateHash is not a set, and inserting the same element twice will change the result.
        a2.insert(ref3);
        assert_ne!(a1, a2);
        a2.remove(ref3);

        a2.insert(ref4);
        assert_ne!(a1, a2);

        // Supports removal.
        a2.remove(ref4);
        assert_eq!(a1, a2);

        // Removing elements out of order should arrive at the same result.
        a2.remove(ref3);
        a2.remove(ref1);

        a1.remove(ref1);
        a1.remove(ref3);

        assert_eq!(a1, a2);

        // After removing all elements, it should be the same as an empty one.
        a1.remove(ref2);
        assert_eq!(a1, ObjectStateHash::default());
    }

    #[test]
    fn test_object_state_hash_commutativity() {
        let ref1 = ObjectDigest::random();
        let ref2 = ObjectDigest::random();
        let ref3 = ObjectDigest::random();

        let mut a1 = ObjectStateHash::default();
        a1.remove(ref1);
        a1.remove(ref2);
        a1.insert(ref1);
        a1.insert(ref2);

        // Removal before insertion should yield the same result
        assert_eq!(a1, ObjectStateHash::default());

        a1.insert(ref1);
        a1.insert(ref2);

        // Insertion out of order should arrive at the same result.
        let mut a2 = ObjectStateHash::default();
        a2.remove(ref1);
        a2.remove(ref2);

        // Unioning where all objects from a are removed in b should
        // result in empty object_state_hash
        a1.union(&a2);
        assert_eq!(a1, ObjectStateHash::default());

        a1.insert(ref1);
        a1.insert(ref2);
        a1.insert(ref3);

        let mut a3 = ObjectStateHash::default();
        a3.insert(ref3);

        // a1: (+ref1, +ref2, +ref3)
        // a2: (-ref1, -ref2)
        // a3: (+ref3)
        // a1 + a2 = a3

        a1.union(&a2);
        assert_eq!(a1, a3);
    }

    #[test]
    fn test_object_state_hash_insert_stress() {
        let mut refs: Vec<_> = (0..100).map(|_| ObjectDigest::random()).collect();
        let mut object_state_hash = ObjectStateHash::default();
        object_state_hash.insert_all(&refs);
        let mut rng = rand::thread_rng();
        (0..10).for_each(|_| {
            refs.shuffle(&mut rng);
            let mut a = ObjectStateHash::default();
            a.insert_all(&refs);
            assert_eq!(object_state_hash, a);
        })
    }

    #[test]
    fn test_object_state_hash_remove_stress() {
        let mut refs1: Vec<_> = (0..100).map(|_| ObjectDigest::random()).collect();
        let mut refs2: Vec<_> = (0..100).map(|_| ObjectDigest::random()).collect();
        let mut object_state_hash = ObjectStateHash::default();
        object_state_hash.insert_all(&refs1);

        let mut rng = rand::thread_rng();
        (0..10).for_each(|_| {
            refs1.shuffle(&mut rng);
            let mut a = ObjectStateHash::default();
            a.insert_all(&refs1);
            a.insert_all(&refs2);
            refs2.shuffle(&mut rng);
            a.remove_all(&refs2);
            assert_eq!(object_state_hash, a);
        })
    }
}
