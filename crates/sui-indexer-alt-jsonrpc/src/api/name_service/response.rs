// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use diesel::{ExpressionMethods, QueryDsl};
use futures::future::OptionFuture;
use sui_indexer_alt_schema::schema::watermarks;
use sui_name_service::{Domain, NameRecord, NameServiceConfig, NameServiceError};
use sui_types::base_types::SuiAddress;
use tokio::join;

use crate::{
    data::objects::load_live,
    error::{invalid_params, RpcError},
    Context,
};

use super::Error;

/// Attempt to to translate the given SuiNS `name` to its address, as long as the mapping exists,
/// and it hasn't expired.
pub(super) async fn resolved_address(
    ctx: &Context,
    config: &NameServiceConfig,
    name: &str,
) -> Result<Option<SuiAddress>, RpcError<Error>> {
    use Error as E;

    let domain: Domain = name
        .parse()
        .map_err(|e| invalid_params(E::NameService(e)))?;

    let domain_record_id = config.record_field_id(&domain);
    let parent_record_id = config.record_field_id(&domain.parent());

    let domain_object = load_live(ctx.loader(), domain_record_id);
    let parent_object: OptionFuture<_> = domain
        .is_subdomain()
        .then(|| load_live(ctx.loader(), parent_record_id))
        .into();

    // Fetch the current timestamp, the domain record. If the domain being resolved is a
    // sub-domain, then also fetch the parent record, because its expiry is controlled by its
    // parent's.
    let (timestamp_ms, domain_object, parent_object) =
        join!(latest_timestamp_ms(ctx), domain_object, parent_object);

    let timestamp_ms = timestamp_ms.context("Failed to fetch latest timestamp")?;

    let Some(domain_object) = domain_object.context("Failed to fetch domain record")? else {
        return Err(invalid_params(E::NotFound(domain.to_string())));
    };

    let domain_record =
        NameRecord::try_from(domain_object).context("Failed to deserialize domain record")?;

    // If the domain being fetched is not a leaf node, then check its expiry directly.
    if !domain_record.is_leaf_record() {
        return if !domain_record.is_node_expired(timestamp_ms) {
            Ok(domain_record.target_address)
        } else {
            return Err(invalid_params(E::NameService(
                NameServiceError::NameExpired,
            )));
        };
    }

    // Otherwise its expiry depends on its parent's record: It must exist, and the domain's record
    // must recognise the fetched parent as its parent, and the parent must not be expired.
    let Some(parent_object) = parent_object
        .transpose()
        .context("Failed to fetch parent record")?
        .flatten()
    else {
        // If the domain object exists but the parent object does not, it could indicate the
        // sub-domain has expired because the parent has been re-registered.
        return Err(invalid_params(E::NotFound(domain.parent().to_string())));
    };

    let parent_record =
        NameRecord::try_from(parent_object).context("Failed to deserialize parent record")?;

    if parent_record.is_valid_leaf_parent(&domain_record)
        && !parent_record.is_node_expired(timestamp_ms)
    {
        Ok(domain_record.target_address)
    } else {
        Err(invalid_params(E::NameService(
            NameServiceError::NameExpired,
        )))
    }
}

/// Fetch the latest timestamp from the database, based on the watermark for the `obj_info`
/// pipeline, because we know that the `obj_info` pipeline is being queried as part of address
/// resolution.
async fn latest_timestamp_ms(ctx: &Context) -> Result<u64, RpcError<Error>> {
    use watermarks::dsl as w;

    let mut conn = ctx
        .reader()
        .connect()
        .await
        .context("Failed to connect to database")?;

    let query = w::watermarks
        .select(w::timestamp_ms_hi_inclusive)
        .filter(w::pipeline.eq("obj_info"));

    let timestamp_ms: i64 = conn
        .first(query)
        .await
        .context("Failed to fetch latest timestamp")?;

    Ok(timestamp_ms as u64)
}
