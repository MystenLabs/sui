// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod gas_cost_summary;
mod ptb_preview;
mod status;
mod summary;

pub struct Pretty<'a, T>(pub &'a T);
