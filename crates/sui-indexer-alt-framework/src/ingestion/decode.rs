// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost::Message;
use sui_rpc::proto::TryFromProtoError;
use sui_rpc::proto::sui::rpc::v2 as proto;

use crate::types::full_checkpoint_content::Checkpoint;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to decompress checkpoint bytes: {0}")]
    Decompression(#[from] std::io::Error),

    #[error("Failed to deserialize checkpoint protobuf: {0}")]
    Deserialization(#[from] prost::DecodeError),

    #[error("Failed to convert checkpoint protobuf to checkpoint data: {0}")]
    ProtoConversion(#[from] TryFromProtoError),
}

impl Error {
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            Self::Decompression(_) => "decompression",
            Self::Deserialization(_) => "deserialization",
            Self::ProtoConversion(_) => "proto_conversion",
        }
    }
}

/// Decode the bytes of a checkpoint from the remote store. The bytes are expected to be a
/// [Checkpoint], represented as a protobuf message, in binary form, zstd-compressed.
pub(crate) fn checkpoint(bytes: &[u8]) -> Result<Checkpoint, Error> {
    let decompressed = zstd::decode_all(bytes)?;
    let proto_checkpoint = proto::Checkpoint::decode(&decompressed[..])?;
    Ok(Checkpoint::try_from(&proto_checkpoint)?)
}
