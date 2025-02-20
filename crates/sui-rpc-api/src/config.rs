// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    /// Enable indexing of transactions and objects
    ///
    /// This enables indexing of transactions and objects which allows for a slightly richer rpc
    /// api. There are some APIs which will be disabled/enabled based on this config while others
    /// (eg GetTransaction) will still be enabled regardless of this config but may return slight
    /// less data (eg GetTransaction won't return the checkpoint that includes the requested
    /// transaction).
    ///
    /// Defaults to `false`, with indexing and APIs which require indexes being disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_indexing: Option<bool>,

    // Only include this till we have another field that isn't set with a non-default value for
    // testing
    #[doc(hidden)]
    #[serde(skip)]
    pub _hidden: (),
}

impl Config {
    pub fn enable_indexing(&self) -> bool {
        self.enable_indexing.unwrap_or(false)
    }
}
