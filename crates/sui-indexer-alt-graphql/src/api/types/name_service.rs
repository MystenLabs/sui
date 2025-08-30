// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use futures::future::OptionFuture;
use sui_name_service::{Domain as NativeDomain, NameRecord, NameServiceConfig};
use sui_types::{base_types::SuiAddress, dynamic_field::Field};
use tokio::join;

use crate::{
    api::scalars::domain::Domain, error::RpcError, scope::Scope, task::watermark::Watermarks,
};

use super::{
    address::Address,
    object::{self, Object, ObjectKey},
};

/// Attempt to translate the given SuiNS `domain` to its address, as long as the mapping exists,
/// and it hasn't expired as of the service's high watermark timestamp.
pub(crate) async fn name_to_address(
    ctx: &Context<'_>,
    scope: &Scope,
    domain: &Domain,
) -> Result<Option<Address>, RpcError<object::Error>> {
    let watermark: &Arc<Watermarks> = ctx.data()?;
    let timestamp_ms = watermark.timestamp_hi_ms();

    let domain_record = name_record(ctx, scope.clone(), domain);
    let parent_record: OptionFuture<_> = domain
        .is_subdomain()
        .then_some(async move { name_record(ctx, scope.clone(), &domain.parent()).await })
        .into();

    // If the domain being resolved is a sub-domain, then also fetch the parent record, because its
    // expiry is controlled by its parent's.
    let (domain_record, parent_record) = join!(domain_record, parent_record);

    let Some(domain_record) = domain_record? else {
        return Ok(None);
    };

    // If the domain being fetched is not a leaf node, then check its expiry directly.
    if !domain_record.is_leaf_record() {
        return Ok(if domain_record.is_node_expired(timestamp_ms) {
            None
        } else {
            domain_record
                .target_address
                .map(|addr| Address::with_address(scope.clone(), addr))
        });
    }

    // Otherwise its expiry depends on its parent's record: It must exist, and the domain's record
    // must recognise the fetched parent as its parent, and the parent must not be expired.
    let Some(parent_record) = parent_record.transpose()?.flatten() else {
        // If the domain object exists but the parent object does not, it could indicate the
        // sub-domain has expired because the parent has been re-registered.
        return Ok(None);
    };

    Ok(
        if parent_record.is_valid_leaf_parent(&domain_record)
            && !parent_record.is_node_expired(timestamp_ms)
        {
            domain_record
                .target_address
                .map(|addr| Address::with_address(scope.clone(), addr))
        } else {
            None
        },
    )
}

/// Attempt to translate the given `address` to its default SuiNS name, as long as the reverse
/// mapping exists, and the corresponding forward mapping is still valid.
pub(crate) async fn address_to_name(
    ctx: &Context<'_>,
    scope: &Scope,
    address: SuiAddress,
) -> Result<Option<String>, RpcError<object::Error>> {
    let Some(reverse_record) = reverse_record(ctx, scope.clone(), address).await? else {
        return Ok(None);
    };

    Ok(name_to_address(ctx, scope, &reverse_record.value)
        .await?
        .is_some()
        .then(|| reverse_record.value.to_string()))
}

/// Fetch the latest version of the name record for the given `domain`.
async fn name_record(
    ctx: &Context<'_>,
    scope: Scope,
    domain: &NativeDomain,
) -> Result<Option<NameRecord>, RpcError<object::Error>> {
    let config: &NameServiceConfig = ctx.data()?;

    let key = ObjectKey {
        address: config.record_field_id(domain).into(),
        version: None,
        root_version: None,
        at_checkpoint: None,
    };

    let Some(object) = Object::by_key(ctx, scope.without_root_version(), key).await? else {
        return Ok(None);
    };

    let Some(object) = object.contents(ctx).await? else {
        return Ok(None);
    };

    let record =
        NameRecord::try_from(object.clone()).context("Failed to deserialize name record")?;

    Ok(Some(record))
}

/// Fetch the latest version of the reverse record for the given `address`.
async fn reverse_record(
    ctx: &Context<'_>,
    scope: Scope,
    address: SuiAddress,
) -> Result<Option<Field<SuiAddress, Domain>>, RpcError<object::Error>> {
    let config: &NameServiceConfig = ctx.data()?;

    let key = ObjectKey {
        address: config.reverse_record_field_id(address.as_ref()).into(),
        version: None,
        root_version: None,
        at_checkpoint: None,
    };

    let Some(object) = Object::by_key(ctx, scope.without_root_version(), key).await? else {
        return Ok(None);
    };

    let Some(object) = object.contents(ctx).await? else {
        return Ok(None);
    };

    let Some(move_object) = object.data.try_as_move() else {
        return Ok(None);
    };

    let record =
        bcs::from_bytes(move_object.contents()).context("Failed to deserialize reverse record")?;

    Ok(Some(record))
}
