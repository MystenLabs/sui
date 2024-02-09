// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod gas_status_displays;
pub mod html_formatter;
mod transaction_displays;
pub struct Pretty<'a, T>(pub &'a T);
