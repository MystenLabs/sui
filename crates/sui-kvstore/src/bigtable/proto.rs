// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::all)]
#![allow(unused)]

#[path = "proto"]
pub mod bigtable {
    #[rustfmt::skip]
    #[path = "google.bigtable.v2.rs"]
    pub mod v2;
}

#[rustfmt::skip]
#[path = "proto/google.rpc.rs"]
pub mod rpc;

#[rustfmt::skip]
#[path = "proto/google.api.rs"]
pub mod api;

#[rustfmt::skip]
#[path = "proto/google.r#type.rs"]
pub mod r#type;
