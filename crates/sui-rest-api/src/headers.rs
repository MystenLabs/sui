// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{headers, http};

pub struct Accept(String);

impl headers::Header for Accept {
    fn name() -> &'static http::HeaderName {
        &axum::http::header::ACCEPT
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
    where
        I: Iterator<Item = &'i http::HeaderValue>,
    {
        let value = values.next().ok_or_else(headers::Error::invalid)?;

        let value = value.to_str().map_err(|_| headers::Error::invalid())?;

        Ok(Self(value.to_owned()))
    }

    fn encode<E>(&self, values: &mut E)
    where
        E: Extend<http::HeaderValue>,
    {
        let value = http::HeaderValue::from_str(&self.0).expect("should be value HeaderValue");

        values.extend(std::iter::once(value));
    }
}

impl Accept {
    pub fn json() -> Self {
        Self(crate::APPLICATION_JSON.to_owned())
    }

    pub fn bcs() -> Self {
        Self(crate::APPLICATION_BCS.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
