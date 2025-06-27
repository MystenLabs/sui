// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::Publication;
use crate::{
    flavor::MoveFlavor,
    schema::{EnvironmentID, EnvironmentName},
};
use move_core_types::account_address::AccountAddress;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Publish information for a package
#[derive(Debug, Serialize, Deserialize)]
pub struct PublishInformation<F: MoveFlavor> {
    /// This is usually the `chain_id`. We need to decide if we really want to abstract the concept of "environments".
    pub environment: EnvironmentID,
    /// The IDs (original, published_at) for the package.
    pub publication: Publication<F>,
}
