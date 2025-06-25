// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods related to local dependencies (of the form `{ local = "<path>" }`)

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{package::paths::PackagePath, schema::OnChainDepInfo};

impl OnChainDepInfo {
    pub fn unfetched_path(&self) -> PathBuf {
        todo!()
    }

    pub async fn fetch(&self) -> PackagePath {
        todo!()
    }
}

/// The constant `true`
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "bool", into = "bool")]
pub struct ConstTrue;

impl TryFrom<bool> for ConstTrue {
    type Error = &'static str;

    fn try_from(value: bool) -> Result<Self, Self::Error> {
        if !value {
            return Err("Expected the constant `true`");
        }
        Ok(Self)
    }
}

impl From<ConstTrue> for bool {
    fn from(value: ConstTrue) -> Self {
        true
    }
}
