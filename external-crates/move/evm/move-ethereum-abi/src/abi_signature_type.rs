// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This file defines JSON-ABI types.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ABIJsonArg {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<ABIJsonArg>>,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct ABIJsonSignature {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub inputs: Vec<ABIJsonArg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<ABIJsonArg>>,
    #[serde(rename = "stateMutability", skip_serializing_if = "Option::is_none")]
    pub state_mutability: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anonymous: Option<bool>,
}
