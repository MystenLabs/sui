// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod sui {
    pub mod rpc {
        pub mod kv {
            pub mod v2alpha {
                include!("generated/sui.rpc.kv.v2alpha.rs");

                /// Byte-encoded FileDescriptorSet for gRPC reflection.
                pub const FILE_DESCRIPTOR_SET: &[u8] =
                    include_bytes!("generated/sui.rpc.kv.v2alpha.fds.bin");
            }
        }
    }
}
