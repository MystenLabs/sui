// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::batch_bls_verifier::*;
use crate::test_utils::{make_cert_with_large_committee, make_dummy_tx};
use futures::future::join_all;
use prometheus::Registry;
use rand::{thread_rng, Rng};
use std::sync::Arc;
use sui_macros::sim_test;
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair, AccountKeyPair, AuthorityKeyPair};
use sui_types::messages::CertifiedTransaction;

fn gen_certs(
    committee: &Committee,
    key_pairs: &[AuthorityKeyPair],
    count: usize,
) -> Vec<CertifiedTransaction> {
    let (receiver, _): (_, AccountKeyPair) = get_key_pair();

    let senders: Vec<_> = (0..count)
        .into_iter()
        .map(|_| get_key_pair::<AccountKeyPair>())
        .collect();

    let txns: Vec<_> = senders
        .iter()
        .map(|(sender, sender_sec)| make_dummy_tx(receiver, *sender, sender_sec))
        .collect();

    txns.iter()
        .map(|t| make_cert_with_large_committee(committee, key_pairs, t))
        .collect()
}

#[sim_test]
async fn test_batch_verify() {
    let (committee, key_pairs) = Committee::new_simple_test_committee();

    let certs = gen_certs(&committee, &key_pairs, 16);

    batch_verify_all_certificates(&committee, &certs).unwrap();

    let (other_sender, other_sender_sec): (_, AccountKeyPair) = get_key_pair();
    // this test is a bit much for the current implementation - it was originally written to verify
    // a bisecting fall back approach.
    for i in 0..16 {
        let (receiver, _): (_, AccountKeyPair) = get_key_pair();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_async_verifier() {
    let (committee, key_pairs) = Committee::new_simple_test_committee();
    let committee = Arc::new(committee);
    let key_pairs = Arc::new(key_pairs);

    let registry = Registry::new();
    let metrics = BatchCertificateVerifierMetrics::new(&registry);
    let verifier = Arc::new(BatchCertificateVerifier::new(
        committee.clone(),
        16,
        metrics,
    ));

    let tasks: Vec<_> = (0..32)
        .into_iter()
        .map(|_| {
            let verifier = verifier.clone();
            let committee = committee.clone();
            let key_pairs = key_pairs.clone();
            tokio::task::spawn(async move {
                let certs = gen_certs(&committee, &key_pairs, 100);

                let (receiver, _): (_, AccountKeyPair) = get_key_pair();
                let (other_sender, other_sender_sec): (_, AccountKeyPair) = get_key_pair();
                let other_tx = make_dummy_tx(receiver, other_sender, &other_sender_sec);
                let other_cert = make_cert_with_large_committee(&committee, &key_pairs, &other_tx);

                for mut c in certs.into_iter() {
                    if thread_rng().gen_range(0..20) == 0 {
                        *c.auth_sig_mut_for_testing() = other_cert.auth_sig().clone();
                        verifier.verify_cert(c).await.unwrap_err();
                    } else {
                        verifier.verify_cert(c).await.unwrap();
                    }
                }
            })
        })
        .collect();

    join_all(tasks).await;
}
