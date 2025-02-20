// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use futures::future;

use super::rpc_module::RpcModule;
use crate::context::Context;
use crate::data::objects::load_latest;
use crate::error::{invalid_params, InternalContext, RpcError};
use crate::paginate::{BcsCursor, Cursor as _, Page};
use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Bool, Bytea, SmallInt};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use move_core_types::language_storage::TypeTag;
use serde::{Deserialize, Serialize};
use sui_indexer_alt_schema::objects::StoredCoinOwnerKind;
use sui_indexer_alt_schema::schema::coin_balance_buckets;
use sui_json_rpc_types::{Coin, Page as PageResponse};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    gas_coin::GAS,
};

#[open_rpc(namespace = "suix", tag = "Coin API")]
#[rpc(server, namespace = "suix")]
trait CoinsApi {
    /// Return Coin objects owned by an address with a specified coin type.
    /// If no coin type is specified, SUI coins are returned.
    #[method(name = "getCoins")]
    async fn get_coins(
        &self,
        /// the owner's Sui address
        owner: SuiAddress,
        /// optional coin type
        coin_type: Option<String>,
        /// optional paging cursor
        cursor: Option<String>,
        /// maximum number of items per page
        limit: Option<usize>,
    ) -> RpcResult<PageResponse<Coin, String>>;
}

pub(crate) struct Coins(pub Context, pub CoinsConfig);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CoinsConfig {
    /// The default page size limit when querying coins, if none is provided.
    pub default_page_size: usize,

    /// The largest acceptable page size when querying coins. Requesting a page larger than
    /// this is a user error.
    pub max_page_size: usize,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Pagination issue: {0}")]
    Pagination(#[from] crate::paginate::Error),

    #[error("Failed to parse type {0:?}: {1}")]
    BadType(String, anyhow::Error),
}

#[derive(Queryable, Debug, Serialize, Deserialize)]
#[diesel(table_name = coin_balance_buckets)]
struct BalanceCursor {
    object_id: Vec<u8>,
    cp_sequence_number: u64,
    coin_balance_bucket: u64,
}

type Cursor = BcsCursor<BalanceCursor>;

#[async_trait::async_trait]
impl CoinsApiServer for Coins {
    async fn get_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: Option<String>,
        limit: Option<usize>,
    ) -> RpcResult<PageResponse<Coin, String>> {
        let coin_type_tag = if let Some(coin_type) = coin_type {
            sui_types::parse_sui_type_tag(&coin_type)
                .map_err(|e| invalid_params(Error::BadType(coin_type, e)))?
        } else {
            GAS::type_tag()
        };

        let Self(ctx, config) = self;

        let page: Page<Cursor> = Page::from_params::<Error>(
            config.default_page_size,
            config.max_page_size,
            cursor,
            limit,
            None,
        )?;

        // We get all the qualified coin ids first.
        let coin_id_page = filter_coins(ctx, owner, coin_type_tag, page).await?;

        let coin_futures = coin_id_page.data.iter().map(|id| coin_response(ctx, *id));

        let coins = future::join_all(coin_futures)
            .await
            .into_iter()
            .zip(coin_id_page.data)
            .map(|(r, id)| r.with_internal_context(|| format!("Failed to get object {id}")))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PageResponse {
            data: coins,
            next_cursor: coin_id_page.next_cursor,
            has_next_page: coin_id_page.has_next_page,
        })
    }
}

impl RpcModule for Coins {
    fn schema(&self) -> Module {
        CoinsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

impl Default for CoinsConfig {
    fn default() -> Self {
        Self {
            default_page_size: 50,
            max_page_size: 100,
        }
    }
}

async fn filter_coins(
    ctx: &Context,
    owner: SuiAddress,
    coin_type_tag: TypeTag,
    page: Page<Cursor>,
) -> Result<PageResponse<ObjectID, String>, RpcError<Error>> {
    use coin_balance_buckets::dsl as cb;

    let mut conn = ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to database")?;

    let limit = page.limit;

    let serialized_coin_type =
        bcs::to_bytes(&coin_type_tag).context("Failed to serialize coin type tag")?;

    // We use two aliases of coin_balance_buckets to make the query more readable.
    let (candidates, newer) = diesel::alias!(
        coin_balance_buckets as candidates,
        coin_balance_buckets as newer
    );

    // Macros to make the query more readable.
    macro_rules! candidates {
        ($field:ident) => {
            candidates.field(cb::$field)
        };
    }

    macro_rules! newer {
        ($field:ident) => {
            newer.field(cb::$field)
        };
    }

    // Construct the basic query first to filter by owner, not deleted and newest rows.
    let mut query = candidates
        .select((
            candidates!(object_id),
            candidates!(cp_sequence_number),
            candidates!(coin_balance_bucket).assume_not_null(),
        ))
        .left_join(
            newer.on(candidates!(object_id)
                .eq(newer!(object_id))
                .and(candidates!(cp_sequence_number).lt(newer!(cp_sequence_number)))),
        )
        .filter(newer!(object_id).is_null())
        .filter(candidates!(owner_kind).eq(StoredCoinOwnerKind::Fastpath))
        .filter(candidates!(owner_id).eq(owner.to_vec()))
        .filter(candidates!(coin_type).eq(serialized_coin_type))
        .into_boxed();

    // If the cursor is specified, we filter by it.
    if let Some(c) = page.cursor {
        query = query.filter(
            sql::<Bool>(r"(candidates.coin_balance_bucket, candidates.cp_sequence_number, candidates.object_id) < (")
                .bind::<SmallInt, _>(c.coin_balance_bucket as i16)
                .sql(", ")
                .bind::<BigInt, _>(c.cp_sequence_number as i64)
                .sql(", ")
                .bind::<Bytea, _>(c.object_id.clone())
                .sql(")")
        );
    }

    // Finally we order by coin_balance_bucket, then by cp_sequence_number, and then by object_id.
    query = query
        .order_by(candidates!(coin_balance_bucket).desc())
        .then_order_by(candidates!(cp_sequence_number).desc())
        .then_order_by(candidates!(object_id).desc())
        .limit(limit + 1);

    let mut buckets: Vec<(Vec<u8>, i64, i16)> =
        conn.results(query).await.context("Failed to query coins")?;

    // Now gather pagination info.
    let has_next_page = buckets.len() > limit as usize;
    if has_next_page {
        buckets.truncate(limit as usize);
    }

    let next_cursor = buckets
        .last()
        .map(|(object_id, cp_sequence_number, coin_balance_bucket)| {
            BcsCursor(BalanceCursor {
                object_id: object_id.clone(),
                cp_sequence_number: *cp_sequence_number as u64,
                coin_balance_bucket: *coin_balance_bucket as u64,
            })
            .encode()
        })
        .transpose()
        .context("Failed to encode cursor")?;

    let ids = buckets
        .iter()
        .map(|(object_id, _, _)| ObjectID::from_bytes(object_id))
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse object id")?;

    Ok(PageResponse {
        data: ids,
        next_cursor,
        has_next_page,
    })
}

async fn coin_response(ctx: &Context, id: ObjectID) -> Result<Coin, RpcError<Error>> {
    let object = load_latest(ctx, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Failed to load latest object {}", id))?;

    let coin_object_id = object.id();
    let digest = object.digest();
    let version = object.version();
    let previous_transaction = object.as_inner().previous_transaction;
    let coin = object
        .as_coin_maybe()
        .context("Object is expected to be a coin")?;
    let coin_type = object
        .coin_type_maybe()
        .context("Object is expected to have a coin type")?
        .to_canonical_string(/* with_prefix */ true);
    Ok(Coin {
        coin_type,
        coin_object_id,
        version,
        digest,
        balance: coin.balance.value(),
        previous_transaction,
    })
}
