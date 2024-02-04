// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod gas_status_displays;
mod transaction_displays;

pub struct Pretty<'a, T>(pub &'a T);
