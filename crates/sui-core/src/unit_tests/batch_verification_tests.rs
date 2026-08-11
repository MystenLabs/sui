// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::signature_verifier::*;
use fastcrypto::traits::KeyPair;
use itertools::Itertools as _;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::committee::Committee;
use sui_types::crypto::AuthorityKeyPair;
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSummary, SignedCheckpointSummary,
};

fn gen_ckpts(
    committee: &Committee,
    key_pairs: &[AuthorityKeyPair],
    count: usize,
) -> Vec<SignedCheckpointSummary> {
    (0..count)
        .map(|i| {
            let k = &key_pairs[i % key_pairs.len()];
            let name = k.public().into();
            SignedCheckpointSummary::new(
                committee.epoch,
                CheckpointSummary::new(
                    &ProtocolConfig::get_for_max_version_UNSAFE(),
                    committee.epoch,
                    // insert different data for each checkpoint so that we can swap sigs later
                    // and get a failure. (otherwise every checkpoint is the same so the
                    // AuthoritySignInfos are interchangeable).
                    i as u64,
                    0,
                    &CheckpointContents::new_with_digests_only_for_tests(vec![]),
                    None,
                    GasCostSummary::default(),
                    None,
                    0,
                    Vec::new(),
                    Vec::new(),
                ),
                k,
                name,
            )
        })
        .collect()
}

#[sim_test]
async fn test_batch_verify_checkpoints() {
    let (committee, key_pairs) = Committee::new_simple_test_committee();

    let ckpts = gen_ckpts(&committee, &key_pairs, 16);
    batch_verify_checkpoints(&committee, &ckpts.iter().collect_vec()).unwrap();

    let mut ckpts = gen_ckpts(&committee, &key_pairs, 16);
    *ckpts[0].auth_sig_mut_for_testing() = ckpts[1].auth_sig().clone();
    batch_verify_checkpoints(&committee, &ckpts.iter().collect_vec()).unwrap_err();
}
