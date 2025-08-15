// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::scalars::sui_address::SuiAddress;
use crate::api::scalars::uint53::UInt53;
use async_graphql::Object;
use sui_types::base_types::ObjectID;
use sui_types::move_package::UpgradeInfo;

pub struct Linkage<'a> {
    pub object_id: &'a ObjectID,
    pub upgrade_info: &'a UpgradeInfo,
}

/// Information used by a package to link to a specific version of its dependency.
#[Object]
impl Linkage<'_> {
    /// The ID on-chain of the first version of the dependency.
    pub(crate) async fn original_id(&self) -> Option<SuiAddress> {
        Some((*self.object_id).into())
    }

    /// The ID on-chain of the version of the dependency that this package depends on.
    pub(crate) async fn upgraded_id(&self) -> Option<SuiAddress> {
        Some(self.upgrade_info.upgraded_id.into())
    }

    /// The version of the dependency that this package depends on.
    pub(crate) async fn version(&self) -> Option<UInt53> {
        Some(self.upgrade_info.upgraded_version.into())
    }
}
