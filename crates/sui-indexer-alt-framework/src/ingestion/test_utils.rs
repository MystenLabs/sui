// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost::Message;
use rand::SeedableRng;
use rand::prelude::StdRng;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc;

use crate::types::crypto::KeypairTraits;
use crate::types::full_checkpoint_content::CheckpointData;
use crate::types::gas::GasCostSummary;
use crate::types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary, SignedCheckpointSummary,
};
use crate::types::supported_protocol_versions::ProtocolConfig;
use crate::types::utils::make_committee_key;

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

    let checkpoint: crate::types::full_checkpoint_content::Checkpoint = checkpoint_data.into();

    let mask = FieldMask::from_paths([
        rpc::v2::Checkpoint::path_builder().sequence_number(),
        rpc::v2::Checkpoint::path_builder().summary().bcs().value(),
        rpc::v2::Checkpoint::path_builder().signature().finish(),
        rpc::v2::Checkpoint::path_builder().contents().bcs().value(),
        rpc::v2::Checkpoint::path_builder()
            .transactions()
            .transaction()
            .bcs()
            .value(),
        rpc::v2::Checkpoint::path_builder()
            .transactions()
            .effects()
            .bcs()
            .value(),
        rpc::v2::Checkpoint::path_builder()
            .transactions()
            .effects()
            .unchanged_loaded_runtime_objects()
            .finish(),
        rpc::v2::Checkpoint::path_builder()
            .transactions()
            .events()
            .bcs()
            .value(),
        rpc::v2::Checkpoint::path_builder()
            .objects()
            .objects()
            .bcs()
            .value(),
    ]);
    let proto_checkpoint = rpc::v2::Checkpoint::merge_from(&checkpoint, &mask.into());

    // Encode to protobuf bytes
    let proto_bytes = proto_checkpoint.encode_to_vec();

    // Compress with zstd
    zstd::encode_all(&proto_bytes[..], 3).unwrap()
}
