// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Canonical control API endpoint definitions shared by server and client.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndpointMethod {
    Get,
    Post,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub path: &'static str,
    pub method: EndpointMethod,
}

impl Endpoint {
    const fn new(path: &'static str, method: EndpointMethod) -> Self {
        Self { path, method }
    }

    pub fn client_path(self) -> &'static str {
        self.path.strip_prefix('/').unwrap_or(self.path)
    }
}

pub(crate) const HEALTH: Endpoint = Endpoint::new("/health", EndpointMethod::Get);
pub(crate) const STATUS: Endpoint = Endpoint::new("/status", EndpointMethod::Get);
