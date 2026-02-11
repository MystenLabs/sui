// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Object;
use futures::future::try_join_all;
use sui_name_service::Domain as NativeDomain;
use sui_name_service::NameRecord as NativeNameRecord;
use sui_name_service::NameServiceConfig;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::dynamic_field::Field;

use crate::api::scalars::uint53::UInt53;
use crate::api::types::address;
use crate::api::types::address::Address;
use crate::api::types::move_type::MoveType;
use crate::api::types::move_value::MoveValue;
use crate::api::types::object;
use crate::api::types::object::Object;
use crate::error::RpcError;
use crate::error::upcast;
use crate::scope::Scope;
use crate::task::watermark::Watermarks;

pub(crate) struct NameRecord {
    pub(crate) super_: MoveValue,
    pub(crate) domain: NativeDomain,
    pub(crate) record: NativeNameRecord,
    pub(crate) ancestors: Vec<(Vec<u8>, NativeNameRecord)>,
}

/// A Name Service NameRecord representing a domain name registration.
#[Object]
impl NameRecord {
    /// The domain name this record is for.
    async fn domain(&self) -> String {
        self.domain.to_string()
    }

    /// On-chain representation of the underlying Name Service `NameRecord` Move value.
    async fn contents(&self) -> &MoveValue {
        &self.super_
    }

    /// The address this domain points to.
    ///
    /// `rootVersion` and `atCheckpoint` control how the target `Address` is scoped. If neither is provided, the `Address` is scoped to the latest checkpoint known to the RPC.
    async fn target(
        &self,
        _ctx: &Context<'_>,
        root_version: Option<UInt53>,
        at_checkpoint: Option<UInt53>,
    ) -> Option<Result<Address, RpcError<address::Error>>> {
        let target_address = self.record.target_address?;

        let scope = if let Some(checkpoint) = at_checkpoint {
            self.scope().with_root_checkpoint(checkpoint.into())
        } else if let Some(version) = root_version {
            self.scope().with_root_version(version.into())
        } else {
            self.scope().clone()
        };

        Some(Ok(Address::with_address(scope, target_address)))
    }

    /// The Name Service Name Record of the parent domain, if this is a subdomain.
    ///
    /// Returns `null` if this is not a subdomain.
    async fn parent(&self) -> Option<NameRecord> {
        let ((native, record), rest) = self.ancestors.split_first()?;

        let super_ = MoveValue {
            type_: self.super_.type_.clone(),
            native: native.clone(),
        };

        Some(NameRecord {
            super_,
            domain: self.domain.parent(),
            record: record.clone(),
            ancestors: rest.to_owned(),
        })
    }
}

impl NameRecord {
    /// Fetch a NameRecord by domain name. Returns `None` if the record does not exist or has
    /// expired.
    pub(crate) async fn by_domain(
        ctx: &Context<'_>,
        scope: Scope,
        domain: NativeDomain,
    ) -> Result<Option<Self>, RpcError<object::Error>> {
        let config: &NameServiceConfig = ctx.data()?;
        let watermark: &Arc<Watermarks> = ctx.data()?;
        let timestamp_ms = watermark.timestamp_hi_ms();

        let domains = std::iter::successors(Some(domain), |domain| {
            domain.is_subdomain().then_some(domain.parent())
        });

        // Fetch all the native records mentioned in the domain chain -- they must all exist.
        let records = try_join_all(domains.map(|d| name_record(ctx, &scope, d))).await?;
        let Some(records) = records.into_iter().collect::<Option<Vec<_>>>() else {
            return Ok(None);
        };

        let mut records = records.into_iter();
        let Some((native, domain, record)) = records.next() else {
            return Ok(None);
        };

        let super_ = MoveValue {
            type_: MoveType::from_native(config.name_record_type().into(), scope),
            native,
        };

        // Check for expiry -- domain record expiries roll up to the nearest non-leaf ancestor. We
        // also capture all the ancestors we consulted, to form the parent chain for the name
        // record.
        let mut ancestors = vec![];
        if !record.is_leaf_record() && record.is_node_expired(timestamp_ms) {
            return Ok(None);
        } else if record.is_leaf_record() {
            for (bytes, _, ancestor) in records {
                if !record.is_valid_leaf_parent(&ancestor) {
                    return Ok(None);
                }

                ancestors.push((bytes, ancestor.clone()));
                if ancestor.is_leaf_record() {
                    continue;
                } else if ancestor.is_node_expired(timestamp_ms) {
                    return Ok(None);
                } else {
                    break;
                }
            }
        }

        Ok(Some(NameRecord {
            super_,
            domain,
            record,
            ancestors,
        }))
    }

    /// Fetch the record corresponding to the default Name Service domain name for the given
    /// `address`, as long as a default mapping exists and its forward mapping is still valid (not
    /// expired).
    pub(crate) async fn by_address(
        ctx: &Context<'_>,
        scope: Scope,
        address: NativeSuiAddress,
    ) -> Result<Option<Self>, RpcError<object::Error>> {
        let Some(reverse) = reverse_record(ctx, scope.clone(), address)
            .await
            .map_err(upcast)?
        else {
            return Ok(None);
        };

        Self::by_domain(ctx, scope, reverse.value).await
    }

    fn scope(&self) -> &Scope {
        &self.super_.type_.scope
    }
}

/// Fetch the latest version of the name record for the given `domain`, returning both the
/// bytes of the on-chain representation, and a deserialized form of the data stored there.
async fn name_record(
    ctx: &Context<'_>,
    scope: &Scope,
    domain: NativeDomain,
) -> Result<Option<(Vec<u8>, NativeDomain, NativeNameRecord)>, RpcError<object::Error>> {
    let config: &NameServiceConfig = ctx.data()?;
    let address = config.record_field_id(&domain).into();

    let Some(object) = Object::latest(ctx, scope.clone(), address)
        .await
        .map_err(upcast)?
    else {
        return Ok(None);
    };

    let Some(contents) = object.contents(ctx).await.map_err(upcast)? else {
        return Ok(None);
    };

    let Some(move_object) = contents.data.try_as_move() else {
        return Ok(None);
    };

    let bytes = move_object.contents().to_owned();
    let field: Field<NativeDomain, NativeNameRecord> =
        bcs::from_bytes(&bytes).context("Failed to deserialize name record")?;

    Ok(Some((bytes, field.name, field.value)))
}

/// Fetch the latest version of the reverse record for the given `address`.
async fn reverse_record(
    ctx: &Context<'_>,
    scope: Scope,
    address: NativeSuiAddress,
) -> Result<Option<Field<NativeSuiAddress, NativeDomain>>, RpcError> {
    let config: &NameServiceConfig = ctx.data()?;

    let address = config.reverse_record_field_id(address.as_ref()).into();
    let Some(object) = Object::latest(ctx, scope.without_root_bound(), address).await? else {
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
