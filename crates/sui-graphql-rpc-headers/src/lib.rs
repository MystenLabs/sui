// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::HeaderName;

pub static VERSION_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-version");
pub static LIMITS_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-show-usage");
