// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]
#![deny(warnings)]

#[macro_use]
pub mod error;

pub mod authority;
pub mod base_types;
pub mod client;
pub mod committee;
pub mod downloader;
pub mod fastpay_smart_contract;
pub mod messages;
pub mod object;
pub mod serialize;
