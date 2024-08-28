// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::Context;

use crate::{
    config::MoveRegistryConfig,
    error::Error,
    types::{move_object::MoveObject, move_package::MovePackage, object::Object},
};

use super::on_chain::{AppInfo, AppRecord, VersionedName};

pub(crate) struct NamedMovePackage;

impl NamedMovePackage {
    /// Queries a package by name (and version, encoded in the name).
    /// Name's format should be `{application}@{organization}/v{versiion}`.
    pub(crate) async fn query(
        ctx: &Context<'_>,
        name: &str,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<MovePackage>, Error> {
        let config: &MoveRegistryConfig = ctx.data_unchecked();
        let versioned = VersionedName::from_str(name)?;

        Self::query_internal(ctx, config, versioned, checkpoint_viewed_at).await
    }

    async fn query_internal(
        ctx: &Context<'_>,
        config: &MoveRegistryConfig,
        versioned: VersionedName,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<MovePackage>, Error> {
        let df_id = versioned.name.to_dynamic_field_id(config).map_err(|e| {
            Error::Internal(format!("Failed to convert name to dynamic field id: {}", e))
        })?;

        let Some(df) =
            MoveObject::query(ctx, df_id.into(), Object::latest_at(checkpoint_viewed_at)).await?
        else {
            return Ok(None);
        };

        let app_record = AppRecord::try_from(df.native)?;

        let Some(app_info) = app_record.app_info else {
            return Ok(None);
        };

        Self::package_from_app_info(ctx, app_info, versioned.version, checkpoint_viewed_at).await
    }

    async fn package_from_app_info(
        ctx: &Context<'_>,
        app_info: AppInfo,
        version: Option<u64>,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<MovePackage>, Error> {
        let Some(package_address) = app_info.package_address else {
            return Ok(None);
        };

        // let's now find the package at a specified version (or latest)
        MovePackage::query(
            ctx,
            package_address.into(),
            if let Some(v) = version {
                MovePackage::by_version(v, checkpoint_viewed_at)
            } else {
                MovePackage::latest_at(checkpoint_viewed_at)
            },
        )
        .await
    }
}
