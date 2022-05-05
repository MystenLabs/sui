// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/// A bincode encoded payload. This is intended to be used in the short-term
/// while we don't have good protobuf definitions for sui types
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BincodeEncodedPayload {
    #[prost(bytes="bytes", tag="1")]
    pub payload: ::prost::bytes::Bytes,
}
