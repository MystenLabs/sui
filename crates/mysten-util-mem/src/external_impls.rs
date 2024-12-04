// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{MallocShallowSizeOf, MallocSizeOf};

// ed25519_consensus
malloc_size_of_is_0!(ed25519_consensus::Signature);

// fastcrypto
malloc_size_of_is_0!(fastcrypto::bls12381::min_sig::BLS12381PublicKey);
malloc_size_of_is_0!(fastcrypto::bls12381::min_sig::BLS12381PublicKeyAsBytes);
malloc_size_of_is_0!(fastcrypto::bls12381::min_sig::BLS12381Signature);
malloc_size_of_is_0!(fastcrypto::bls12381::min_sig::BLS12381AggregateSignature);
malloc_size_of_is_0!(fastcrypto::bls12381::min_sig::BLS12381AggregateSignatureAsBytes);
malloc_size_of_is_0!(fastcrypto::bls12381::min_pk::BLS12381PublicKey);
malloc_size_of_is_0!(fastcrypto::bls12381::min_pk::BLS12381Signature);
malloc_size_of_is_0!(fastcrypto::bls12381::min_pk::BLS12381AggregateSignature);
malloc_size_of_is_0!(fastcrypto::ed25519::Ed25519PublicKey);
malloc_size_of_is_0!(fastcrypto::ed25519::Ed25519Signature);
impl MallocSizeOf for fastcrypto::ed25519::Ed25519AggregateSignature {
    fn size_of(&self, ops: &mut crate::MallocSizeOfOps) -> usize {
        self.sigs.size_of(ops)
    }
}
malloc_size_of_is_0!(fastcrypto::groups::ristretto255::RistrettoPoint);
impl<G> MallocSizeOf for fastcrypto_tbls::dkg_v1::Complaint<G>
where
    G: fastcrypto::groups::GroupElement,
{
    fn size_of(&self, _ops: &mut crate::MallocSizeOfOps) -> usize {
        0
    }
}

impl<G> MallocSizeOf for fastcrypto_tbls::dkg_v1::Confirmation<G>
where
    G: fastcrypto::groups::GroupElement,
{
    fn size_of(&self, ops: &mut crate::MallocSizeOfOps) -> usize {
        self.complaints.size_of(ops)
    }
}
impl<G, EG> MallocSizeOf for fastcrypto_tbls::dkg_v1::Message<G, EG>
where
    G: fastcrypto::groups::GroupElement,
    EG: fastcrypto::groups::GroupElement,
{
    fn size_of(&self, ops: &mut crate::MallocSizeOfOps) -> usize {
        self.encrypted_shares.size_of(ops)
    }
}
impl<G> MallocSizeOf for fastcrypto_tbls::ecies_v1::MultiRecipientEncryption<G>
where
    G: fastcrypto::groups::GroupElement,
{
    fn size_of(&self, _ops: &mut crate::MallocSizeOfOps) -> usize {
        // Can't measure size of internal Vec<Vec<u8>> here because it's private.
        0
    }
}
impl MallocSizeOf for fastcrypto_tbls::polynomial::Poly<fastcrypto::groups::bls12381::G2Element> {
    fn size_of(&self, _ops: &mut crate::MallocSizeOfOps) -> usize {
        (self.degree() as usize + 1)
            * std::mem::size_of::<fastcrypto::groups::bls12381::G2Element>()
    }
}
malloc_size_of_is_0!(fastcrypto::groups::bls12381::G1Element);

// hash_map
malloc_size_of_is_0!(std::collections::hash_map::RandomState);

// indexmap
impl<K: MallocSizeOf, V: MallocSizeOf, S> MallocShallowSizeOf for indexmap::IndexMap<K, V, S> {
    fn shallow_size_of(&self, _ops: &mut crate::MallocSizeOfOps) -> usize {
        self.capacity()
            * (std::mem::size_of::<K>()
                + std::mem::size_of::<V>()
                + (2 * std::mem::size_of::<usize>()))
    }
}
impl<K: MallocSizeOf, V: MallocSizeOf, S> MallocSizeOf for indexmap::IndexMap<K, V, S> {
    // This only produces a rough estimate of IndexMap size, because we cannot access private
    // fields to measure them precisely.
    fn size_of(&self, ops: &mut crate::MallocSizeOfOps) -> usize {
        let mut n = self.shallow_size_of(ops);
        if let (Some(k), Some(v)) = (K::constant_size(), V::constant_size()) {
            n += self.len() * (k + v)
        } else {
            n += self
                .iter()
                .fold(n, |acc, (k, v)| acc + k.size_of(ops) + v.size_of(ops))
        }
        n
    }
}

// roaring
impl MallocSizeOf for roaring::RoaringBitmap {
    // This only produces a rough estimate of RoaringBitmap size, because we cannot access private
    // fields to measure them precisely.
    fn size_of(&self, _ops: &mut crate::MallocSizeOfOps) -> usize {
        self.serialized_size()
    }
}
