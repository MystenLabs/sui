// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A JSONRPC module implementation. The methods it serves are described by the frozen OpenRPC
/// document at the crate root (`openrpc.json`), which is served via `rpc.discover`.
pub trait RpcModule: Sized {
    /// The implementation of the JSONRPC module.
    fn into_impl(self) -> jsonrpsee::RpcModule<Self>;
}
