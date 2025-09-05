// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use crate::api::scalars::{base64::Base64, sui_address::SuiAddress};

use super::TransactionArgument;

/// Upgrades a Move Package.
#[derive(SimpleObject, Clone)]
pub struct UpgradeCommand {
    /// Bytecode for the modules to be published, BCS serialized and Base64 encoded.
    pub modules: Option<Vec<Base64>>,

    /// IDs of the transitive dependencies of the package to be published.
    pub dependencies: Option<Vec<SuiAddress>>,

    /// ID of the package being upgraded.
    pub current_package: Option<SuiAddress>,

    /// The `UpgradeTicket` authorizing the upgrade.
    pub upgrade_ticket: Option<TransactionArgument>,
}
