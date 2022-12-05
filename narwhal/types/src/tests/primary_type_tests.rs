// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::WorkerId;
use crypto::{KeyPair, Signature};
use fastcrypto::{
    hash::{Digest, Hash},
    traits::KeyPair as _,
};
use once_cell::sync::OnceCell;
use proptest::{collection, prelude::*, strategy::Strategy};
use rand::{rngs::StdRng, SeedableRng};
use signature::Signer;

use crate::{BatchDigest, CertificateDigest, Header, HeaderDigest};

fn arb_keypair() -> impl Strategy<Value = KeyPair> {
    (any::<[u8; 32]>())
        .prop_map(|rand| {
            let mut rng = StdRng::from_seed(rand);
            KeyPair::generate(&mut rng)
        })
        .no_shrink()
}

fn clean_signed_header(kp: KeyPair) -> impl Strategy<Value = Header> {
    (
        any::<u64>(),
        any::<u64>(),
        collection::vec((BatchDigest::arbitrary(), any::<u32>()), 0..10),
        collection::vec(CertificateDigest::arbitrary(), 0..10),
    )
        .prop_map(move |(round, epoch, batches, parents)| {
            let payload = batches
                .into_iter()
                .map(|(batch_digest, worker_id)| (batch_digest, worker_id as WorkerId))
                .collect();

            let parents = parents.into_iter().collect();

            let header = Header {
                author: kp.public().clone(),
                round,
                epoch,
                created_at: 0,
                payload,
                parents,
                digest: OnceCell::default(),
                signature: Signature::default(),
            };
            Header {
                digest: OnceCell::with_value(Hash::digest(&header)),
                signature: kp.sign(Digest::from(Hash::digest(&header)).as_ref()),
                ..header
            }
        })
}

fn arb_signed_header(kp: KeyPair) -> impl Strategy<Value = Header> {
    (
        clean_signed_header(kp.copy()),
        HeaderDigest::arbitrary(),
        any::<usize>(),
    )
        .prop_map(move |(clean_header, random_digest, naughtiness)| {
            let naughtiness = naughtiness % 100;
            if naughtiness < 95 {
                clean_header
            } else if naughtiness < 99 {
                let signature = kp.sign(Digest::from(random_digest).as_ref());
                // naughty: we provide a well-signed random header
                Header {
                    digest: OnceCell::with_value(random_digest),
                    signature,
                    ..clean_header
                }
            } else {
                // naughty: we provide an ill-signed random header
                Header {
                    digest: OnceCell::with_value(random_digest),
                    ..clean_header
                }
            }
        })
}

fn arb_header() -> impl Strategy<Value = Header> {
    arb_keypair().prop_flat_map(arb_signed_header)
}

proptest! {
    #[test]
    fn header_deserializes_to_correct_id(header in arb_header()) {
        let serialized = bincode::serialize(&header).unwrap();
        let deserialized: Header = bincode::deserialize(&serialized).unwrap();
        // We may not have header.digest() == Hash::digest(header), due to the naughty cases above.
        //
        // Indeed, the naughty headers are specially crafted so that their `digest` is populated with a wrong digest.
        // They are malformed, since a correct header always has `foo.digest() == Hash::digest(foo)`.
        // We check here that deserializing a header, even a malformed one,
        // produces a correctly-formed header in all cases.
        assert_eq!(deserialized.digest(), Hash::digest(&header));
    }

}
