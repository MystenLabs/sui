// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::Context;

use crate::api::types::available_range::AvailableRangeKey;
use crate::error::RpcError;
use crate::error::upcast;
use crate::task::watermark::Watermarks;

/// Lets a single gating helper short-circuit a resolver method regardless of whether its return
/// type is `Result<T, RpcError<E>>` or `Option<Result<T, RpcError<E>>>` (the two shapes used by
/// every resolver in this crate). Implemented for both shapes below; `#[GatedObject]` relies on
/// return-type-directed inference to select the right impl.
pub(crate) trait GatedResolverResult {
    fn from_pipeline_error(err: RpcError) -> Self;
}

impl<T, E: std::error::Error> GatedResolverResult for Result<T, RpcError<E>> {
    fn from_pipeline_error(err: RpcError) -> Self {
        Err(upcast(err))
    }
}

impl<T, E: std::error::Error> GatedResolverResult for Option<Result<T, RpcError<E>>> {
    fn from_pipeline_error(err: RpcError) -> Self {
        Some(Err(upcast(err)))
    }
}

/// Checks that every pipeline backing `type_`'s `field` (ignoring active filters -- fields whose
/// pipeline requirement depends on them get the unfiltered/base set) is present in the
/// `Watermarks` found in `ctx`.
///
/// Called from the code that `#[sui_indexer_alt_graphql_macros::GatedObject]` (see the
/// `sui-indexer-alt-graphql-macros` crate) injects into the start of every resolver method that
/// takes `ctx: &Context<'_>`.
pub(crate) fn check_pipeline_available(
    ctx: &Context<'_>,
    type_: &str,
    field: &str,
) -> Result<(), RpcError> {
    let watermarks: &Arc<Watermarks> = ctx.data()?;
    AvailableRangeKey {
        type_: type_.to_owned(),
        field: Some(field.to_owned()),
        filters: None,
    }
    .reader_lo(watermarks)?;
    Ok(())
}
