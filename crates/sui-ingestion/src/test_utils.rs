// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::prelude::StdRng;
use rand::SeedableRng;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::crypto::KeypairTraits;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary, SignedCheckpointSummary,
};
use sui_types::supported_protocol_versions::ProtocolConfig;
use sui_types::utils::make_committee_key;

const RNG_SEED: [u8; 32] = [
    21, 23, 199, 200, 234, 250, 252, 178, 94, 15, 202, 178, 62, 186, 88, 137, 233, 192, 130, 157,
    179, 179, 65, 9, 31, 249, 221, 123, 225, 112, 199, 247,
];

pub(crate) fn test_checkpoint_data(cp: u64) -> Vec<u8> {
    let mut rng = StdRng::from_seed(RNG_SEED);
    let (keys, committee) = make_committee_key(&mut rng);
    let contents = CheckpointContents::new_with_digests_only_for_tests(vec![]);
    let summary = CheckpointSummary::new(
        &ProtocolConfig::get_for_max_version_UNSAFE(),
        0,
        cp,
        0,
        &contents,
        None,
        GasCostSummary::default(),
        None,
        0,
        Vec::new(),
    );

    let sign_infos: Vec<_> = keys
        .iter()
        .map(|k| {
            let name = k.public().into();
            SignedCheckpointSummary::sign(committee.epoch, &summary, k, name)
        })
        .collect();

    let checkpoint_data = CheckpointData {
        checkpoint_summary: CertifiedCheckpointSummary::new(summary, sign_infos, &committee)
            .unwrap(),
        checkpoint_contents: contents,
        transactions: vec![],
    };

    Blob::encode(&checkpoint_data, BlobEncoding::Bcs)
        .unwrap()
        .to_bytes()
}
