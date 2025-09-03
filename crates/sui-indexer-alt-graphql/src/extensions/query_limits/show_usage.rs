// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::{HeaderName, HeaderValue};
use headers::{Error, Header};

static SHOW_USAGE: HeaderName = HeaderName::from_static("x-sui-rpc-show-usage");

/// Header indicating that the client would like their GraphQL response to be extended with how
/// much of each limit the query is using. The value of this header doesn't matter, it just has to
/// be present.
pub(crate) struct ShowUsage(pub HeaderValue);

impl Header for ShowUsage {
    fn name() -> &'static HeaderName {
        &SHOW_USAGE
    }

    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(values: &mut I) -> Result<Self, Error> {
        Ok(ShowUsage(values.next().ok_or_else(Error::invalid)?.clone()))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.clone()]);
    }
}
