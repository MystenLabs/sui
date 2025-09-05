// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Include the generated proto definitions.
include!("../../../generated/sui.rpc.consistent.v1alpha.rs");

// Byte encoded `FILE_DESCRIPTOR_SET`.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!("../../../generated/sui.rpc.consistent.v1alpha.fds.bin");

/// Metadata name used in requests to set the checkpoint to make the request at, and in responses
/// to indicate the checkpoint at which the response was generated.
pub const CHECKPOINT_METADATA: &str = "x-sui-checkpoint";

#[cfg(test)]
mod tests {
    use super::FILE_DESCRIPTOR_SET;
    use prost::Message as _;

    #[test]
    fn file_descriptor_set_is_valid() {
        prost_types::FileDescriptorSet::decode(FILE_DESCRIPTOR_SET).unwrap();
    }
}
