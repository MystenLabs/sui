// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{EnvironmentID, EnvironmentName};
use move_core_types::account_address::AccountAddress;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type PublishInformationMap = BTreeMap<EnvironmentName, PublishInformation>;

/// Publish information for a package
#[derive(Debug, Serialize, Deserialize)]
pub struct PublishInformation {
    /// This is usually the `chain_id`. We need to decide if we really want to abstract the concept of "environments".
    pub environment: EnvironmentID,
    /// The IDs (original, published_at) for the package.
    #[serde(flatten)]
    pub published_ids: PublishedIds,
    /// The current version of the package -- this info is not needed in the package graphs, maybe
    /// helps with conflict errors handling.
    pub version: String,
}

/// The published IDs for a package from the lockfile
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PublishedIds {
    /// The "original" address (v1 of the published package)
    pub original_id: AccountAddress,
    /// The `latest` address (latest address of the published package)
    pub latest_id: AccountAddress,
}
