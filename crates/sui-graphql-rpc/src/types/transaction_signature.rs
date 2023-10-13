// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;

// TODO: flesh out the scalar transaction signature
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct TransactionSignature {
    pub base64_sig: String,
}
