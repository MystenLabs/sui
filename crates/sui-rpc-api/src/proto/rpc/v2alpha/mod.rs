// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Include the generated proto definitions
include!("../../generated/sui.rpc.v2alpha.rs");

/// Byte encoded FILE_DESCRIPTOR_SET.
pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("../../generated/sui.rpc.v2alpha.fds.bin");

#[cfg(test)]
mod tests {
    use super::FILE_DESCRIPTOR_SET;
    use prost::Message as _;

    #[test]
    fn file_descriptor_set_is_valid() {
        prost_types::FileDescriptorSet::decode(FILE_DESCRIPTOR_SET).unwrap();
    }
}
