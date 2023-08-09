// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use serde::{Deserialize, Serialize};

const SUI_ADDRESS_LENGTH: usize = 32;

#[derive(Serialize, Deserialize)]
struct SuiAddress([u8; SUI_ADDRESS_LENGTH]);

scalar!(SuiAddress, "SuiAddress", "Representation of Sui Addresses");
