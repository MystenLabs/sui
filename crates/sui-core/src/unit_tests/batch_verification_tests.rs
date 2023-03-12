// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::committee::Committee;

use crate::batch_bls_verifier::*;
use crate::test_utils::{make_cert_with_large_committee, make_dummy_tx};
use sui_macros::sim_test;
use sui_types::crypto::{get_key_pair, AccountKeyPair};

#[sim_test]
async fn test_batch_verify() {
    let (committee, key_pairs) = Committee::new_simple_test_committee();

    let (receiver, _): (_, AccountKeyPair) = get_key_pair();

    let senders: Vec<_> = (0..16)
        .into_iter()
        .map(|_| get_key_pair::<AccountKeyPair>())
        .collect();

    let txns: Vec<_> = senders
        .iter()
        .map(|(sender, sender_sec)| make_dummy_tx(receiver, *sender, sender_sec))
        .collect();

    let certs: Vec<_> = txns
        .iter()
        .map(|t| make_cert_with_large_committee(&committee, &key_pairs, t))
        .collect();

    batch_verify_all_certificates(&committee, &certs).unwrap();

    let (other_sender, other_sender_sec): (_, AccountKeyPair) = get_key_pair();
    // this test is a bit much for the current implementation - it was originally written to verify
    // a bisecting fall back approach.
    for i in 0..16 {
        let mut certs = certs.clone();
        let other_tx = make_dummy_tx(receiver, other_sender, &other_sender_sec);
        let other_cert = make_cert_with_large_committee(&committee, &key_pairs, &other_tx);
        *certs[i].auth_sig_mut_for_testing() = other_cert.auth_sig().clone();
        batch_verify_all_certificates(&committee, &certs).unwrap_err();

        let results = batch_verify_certificates(&committee, &certs);
        results[i].as_ref().unwrap_err();
        for (_, r) in results.iter().enumerate().filter(|(j, _)| *j != i) {
            r.as_ref().unwrap();
        }
    }
}
