// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;

use super::base64::Base64;

// TODO: flesh out the scalar transaction signature
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct TransactionSignature {
    pub base64_sig: Base64,
}
