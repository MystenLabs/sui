// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::Context;

use crate::{
    error::Error,
    types::{
        chain_identifier::ChainIdentifier, move_object::MoveObject, move_package::MovePackage,
        object::Object,
    },
};

use super::{
    config::{
        AppInfo, AppRecord, DotMoveConfig, DotMoveServiceError, ResolutionType, VersionedName,
    },
    data_loader::DotMoveDataLoader,
};

pub(crate) struct NamedMovePackage;

impl NamedMovePackage {
    pub(crate) async fn query(
        ctx: &Context<'_>,
        name: &str,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<MovePackage>, Error> {
        let config: &DotMoveConfig = ctx.data_unchecked();
        let versioned = VersionedName::from_str(name)?;

        // Non-base chain id handling for name resolution (uses external api to resolve names).
        if config.resolution_type == ResolutionType::Internal {
            Self::query_internal(ctx, config, versioned, checkpoint_viewed_at).await
        } else {
            Self::query_external(ctx, config, versioned, checkpoint_viewed_at).await
        }
    }

    async fn query_internal(
        ctx: &Context<'_>,
        config: &DotMoveConfig,
        versioned: VersionedName,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<MovePackage>, Error> {
        let Some(df) = MoveObject::query(
            ctx,
            versioned.name.to_dynamic_field_id(config).into(),
            Object::latest_at(checkpoint_viewed_at),
        )
        .await?
        else {
            return Ok(None);
        };

        let app_record = AppRecord::try_from(df.native)?;

        let Some(app_info) = app_record.app_info else {
            return Ok(None);
        };

        Self::package_from_app_info(ctx, app_info, versioned.version, checkpoint_viewed_at).await
    }

    async fn query_external(
        ctx: &Context<'_>,
        config: &DotMoveConfig,
        versioned: VersionedName,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<MovePackage>, Error> {
        if config.mainnet_api_url.is_none() {
            return Err(DotMoveServiceError::MainnetApiUrlUnavailable.into());
        }

        let identifier: ChainIdentifier = *ctx.data_unchecked();

        let Some(chain_id) = identifier.0 else {
            return Err(DotMoveServiceError::ChainIdentifierUnavailable.into());
        };

        let DotMoveDataLoader(loader) = &ctx.data_unchecked();

        let Some(result) = loader.load_one(versioned.name).await? else {
            return Ok(None);
        };

        let Some(app_info) = result.networks.get(&chain_id.to_string()) else {
            return Ok(None);
        };

        Self::package_from_app_info(
            ctx,
            app_info.clone(),
            versioned.version,
            checkpoint_viewed_at,
        )
        .await
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
            version.map_or(MovePackage::latest_at(checkpoint_viewed_at), |v| {
                MovePackage::by_version(v, checkpoint_viewed_at)
            }),
        )
        .await
    }
}
