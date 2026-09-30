// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::HeaderName;
use axum::http::HeaderValue;
use headers::Error;
use headers::Header;
use sui_indexer_alt_reader::fullnode_client::X_SUI_CLIENT_PROTOCOL_VERSION;

static CLIENT_PROTOCOL_VERSION: HeaderName = HeaderName::from_static(X_SUI_CLIENT_PROTOCOL_VERSION);

/// The caller's decoding capabilities, forwarded for the fullnode to interpret.
pub(crate) struct ClientProtocolVersion(pub HeaderValue);

impl Header for ClientProtocolVersion {
    fn name() -> &'static HeaderName {
        &CLIENT_PROTOCOL_VERSION
    }

    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(values: &mut I) -> Result<Self, Error> {
        Ok(Self(values.next().ok_or_else(Error::invalid)?.clone()))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.clone()]);
    }
}
