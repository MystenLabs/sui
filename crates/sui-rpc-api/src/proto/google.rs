// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod protobuf {
    /// Byte encoded FILE_DESCRIPTOR_SET.
    pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("generated/google.protobuf.fds.bin");

    #[cfg(test)]
    mod tests {
        use super::FILE_DESCRIPTOR_SET;
        use prost::Message as _;

        #[test]
        fn file_descriptor_set_is_valid() {
            prost_types::FileDescriptorSet::decode(FILE_DESCRIPTOR_SET).unwrap();
        }
    }
}

pub mod rpc {
    include!("generated/google.rpc.rs");

    /// Byte encoded FILE_DESCRIPTOR_SET.
    pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("generated/google.rpc.fds.bin");

    impl ::prost::Name for Status {
        const NAME: &'static str = "Status";
        const PACKAGE: &'static str = "google.rpc";
        fn full_name() -> ::prost::alloc::string::String {
            "google.rpc.Status".into()
        }
        fn type_url() -> ::prost::alloc::string::String {
            "/google.rpc.Status".into()
        }
    }

    impl ::prost::Name for ErrorInfo {
        const NAME: &'static str = "ErrorInfo";
        const PACKAGE: &'static str = "google.rpc";
        fn full_name() -> ::prost::alloc::string::String {
            "google.rpc.ErrorInfo".into()
        }
        fn type_url() -> ::prost::alloc::string::String {
            "/google.rpc.ErrorInfo".into()
        }
    }
    impl ::prost::Name for RetryInfo {
        const NAME: &'static str = "RetryInfo";
        const PACKAGE: &'static str = "google.rpc";
        fn full_name() -> ::prost::alloc::string::String {
            "google.rpc.RetryInfo".into()
        }
        fn type_url() -> ::prost::alloc::string::String {
            "/google.rpc.RetryInfo".into()
        }
    }
    impl ::prost::Name for DebugInfo {
        const NAME: &'static str = "DebugInfo";
        const PACKAGE: &'static str = "google.rpc";
        fn full_name() -> ::prost::alloc::string::String {
            "google.rpc.DebugInfo".into()
        }
        fn type_url() -> ::prost::alloc::string::String {
            "/google.rpc.DebugInfo".into()
        }
    }
    impl ::prost::Name for QuotaFailure {
        const NAME: &'static str = "QuotaFailure";
        const PACKAGE: &'static str = "google.rpc";
        fn full_name() -> ::prost::alloc::string::String {
            "google.rpc.QuotaFailure".into()
        }
        fn type_url() -> ::prost::alloc::string::String {
            "/google.rpc.QuotaFailure".into()
        }
    }
    impl ::prost::Name for PreconditionFailure {
        const NAME: &'static str = "PreconditionFailure";
        const PACKAGE: &'static str = "google.rpc";
        fn full_name() -> ::prost::alloc::string::String {
            "google.rpc.PreconditionFailure".into()
        }
        fn type_url() -> ::prost::alloc::string::String {
            "/google.rpc.PreconditionFailure".into()
        }
    }
    impl ::prost::Name for BadRequest {
        const NAME: &'static str = "BadRequest";
        const PACKAGE: &'static str = "google.rpc";
        fn full_name() -> ::prost::alloc::string::String {
            "google.rpc.BadRequest".into()
        }
        fn type_url() -> ::prost::alloc::string::String {
            "/google.rpc.BadRequest".into()
        }
    }
    impl ::prost::Name for RequestInfo {
        const NAME: &'static str = "RequestInfo";
        const PACKAGE: &'static str = "google.rpc";
        fn full_name() -> ::prost::alloc::string::String {
            "google.rpc.RequestInfo".into()
        }
        fn type_url() -> ::prost::alloc::string::String {
            "/google.rpc.RequestInfo".into()
        }
    }
    impl ::prost::Name for ResourceInfo {
        const NAME: &'static str = "ResourceInfo";
        const PACKAGE: &'static str = "google.rpc";
        fn full_name() -> ::prost::alloc::string::String {
            "google.rpc.ResourceInfo".into()
        }
        fn type_url() -> ::prost::alloc::string::String {
            "/google.rpc.ResourceInfo".into()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::FILE_DESCRIPTOR_SET;
        use prost::Message as _;

        #[test]
        fn file_descriptor_set_is_valid() {
            prost_types::FileDescriptorSet::decode(FILE_DESCRIPTOR_SET).unwrap();
        }
    }
}
