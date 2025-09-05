// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use crate::api::scalars::base64::Base64;

/// BCS encoded primitive value (not an object or Move struct).
#[derive(SimpleObject, Clone)]
pub struct Pure {
    /// BCS serialized and Base64 encoded primitive value.
    pub bytes: Option<Base64>,
}
