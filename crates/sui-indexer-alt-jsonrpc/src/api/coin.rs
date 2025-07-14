// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::Context as _;
use diesel::prelude::*;
use diesel::sql_types::Bool;
use futures::future;
use jsonrpsee::{core::RpcResult, http_client::HttpClient, proc_macros::rpc};
use move_core_types::language_storage::{StructTag, TypeTag};
use serde::{Deserialize, Serialize};
use sui_indexer_alt_reader::coin_metadata::CoinMetadataKey;
use sui_indexer_alt_schema::objects::StoredCoinOwnerKind;
use sui_indexer_alt_schema::schema::coin_balance_buckets;
use sui_json_rpc_types::{Balance, Coin, Page as PageResponse, SuiCoinMetadata};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_sql_macro::sql;
use sui_types::object::Object;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    gas_coin::GAS,
};

use crate::{
    config::NodeConfig,
    context::Context,
    data::load_live,
    error::{client_error_to_error_object, invalid_params, InternalContext, RpcError},
    paginate::{BcsCursor, Cursor as _, Page},
};

use super::rpc_module::RpcModule;

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

    /// Return metadata (e.g., symbol, decimals) for a coin. Note that if the coin's metadata was
    /// wrapped in the transaction that published its marker type, or the latest version of the
    /// metadata object is wrapped or deleted, it will not be found.
    #[method(name = "getCoinMetadata")]
    async fn get_coin_metadata(
        &self,
        /// type name for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC)
        coin_type: String,
    ) -> RpcResult<Option<SuiCoinMetadata>>;
}

/// Delegation Coin API for endpoints that are delegated to FN RPC
#[open_rpc(namespace = "suix", tag = "Delegation Coin API")]
#[rpc(server, client, namespace = "suix")]
trait DelegationCoinsApi {
    /// Return the total coin balance for all coin types, owned by the address owner.
    #[method(name = "getAllBalances")]
    async fn get_all_balances(
        &self,
        /// the owner's Sui address
        owner: SuiAddress,
    ) -> RpcResult<Vec<Balance>>;

    /// Return the total coin balance for one coin type, owned by the address.
    /// If no coin type is specified, SUI coin balance is returned.
    #[method(name = "getBalance")]
    async fn get_balance(
        &self,
        /// the owner's Sui address
        owner: SuiAddress,
        /// optional type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC), default to 0x2::sui::SUI if not specified.
        coin_type: Option<String>,
    ) -> RpcResult<Balance>;
}

pub(crate) struct Coins(pub Context);
pub(crate) struct DelegationCoins(HttpClient);

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

impl DelegationCoins {
    pub fn new(fullnode_rpc_url: url::Url, config: NodeConfig) -> anyhow::Result<Self> {
        let client = config.client(fullnode_rpc_url)?;
        Ok(Self(client))
    }
}

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

        let Self(ctx) = self;
        let config = &ctx.config().coins;

        let page: Page<Cursor> = Page::from_params::<Error>(
            config.default_page_size,
            config.max_page_size,
            cursor,
            limit,
            None,
        )?;

        // We get all the qualified coin ids first.
        let coin_id_page = filter_coins(ctx, owner, Some(coin_type_tag), Some(page)).await?;

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

    async fn get_coin_metadata(&self, coin_type: String) -> RpcResult<Option<SuiCoinMetadata>> {
        let Self(ctx) = self;

        Ok(coin_metadata_response(ctx, &coin_type)
            .await
            .with_internal_context(|| format!("Failed to fetch CoinMetadata for {coin_type:?}"))?)
    }
}

#[async_trait::async_trait]
impl DelegationCoinsApiServer for DelegationCoins {
    async fn get_all_balances(&self, owner: SuiAddress) -> RpcResult<Vec<Balance>> {
        let Self(client) = self;

        client
            .get_all_balances(owner)
            .await
            .map_err(client_error_to_error_object)
    }

    async fn get_balance(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> RpcResult<Balance> {
        let Self(client) = self;

        client
            .get_balance(owner, coin_type)
            .await
            .map_err(client_error_to_error_object)
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

impl RpcModule for DelegationCoins {
    fn schema(&self) -> Module {
        DelegationCoinsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

async fn filter_coins(
    ctx: &Context,
    owner: SuiAddress,
    coin_type_tag: Option<TypeTag>,
    page: Option<Page<Cursor>>,
) -> Result<PageResponse<ObjectID, String>, RpcError<Error>> {
    use coin_balance_buckets::dsl as cb;

    let mut conn = ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to database")?;

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
        .into_boxed();

    if let Some(coin_type_tag) = coin_type_tag {
        let serialized_coin_type =
            bcs::to_bytes(&coin_type_tag).context("Failed to serialize coin type tag")?;
        query = query.filter(candidates!(coin_type).eq(serialized_coin_type));
    }

    let (cursor, limit) = page.map_or((None, None), |p| (p.cursor, Some(p.limit)));

    // If the cursor is specified, we filter by it.
    if let Some(c) = cursor {
        query = query.filter(sql!(as Bool,
            "(candidates.coin_balance_bucket, candidates.cp_sequence_number, candidates.object_id) < ({SmallInt}, {BigInt}, {Bytea})",
            c.coin_balance_bucket as i16,
            c.cp_sequence_number as i64,
            c.object_id.clone(),
        ));
    }

    // Finally we order by coin_balance_bucket, then by cp_sequence_number, and then by object_id.
    query = query
        .order_by(candidates!(coin_balance_bucket).desc())
        .then_order_by(candidates!(cp_sequence_number).desc())
        .then_order_by(candidates!(object_id).desc());

    if let Some(limit) = limit {
        query = query.limit(limit + 1);
    }

    let mut buckets: Vec<(Vec<u8>, i64, i16)> =
        conn.results(query).await.context("Failed to query coins")?;

    let mut has_next_page = false;

    if let Some(limit) = limit {
        // Now gather pagination info.
        has_next_page = buckets.len() > limit as usize;
        if has_next_page {
            buckets.truncate(limit as usize);
        }
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
    let (object, coin_type, balance) = object_with_coin_data(ctx, id).await?;

    let coin_object_id = object.id();
    let digest = object.digest();
    let version = object.version();
    let previous_transaction = object.as_inner().previous_transaction;

    Ok(Coin {
        coin_type,
        coin_object_id,
        version,
        digest,
        balance,
        previous_transaction,
    })
}

async fn coin_metadata_response(
    ctx: &Context,
    coin_type: &str,
) -> Result<Option<SuiCoinMetadata>, RpcError<Error>> {
    let coin_type = StructTag::from_str(coin_type)
        .map_err(|e| invalid_params(Error::BadType(coin_type.to_owned(), e)))?;

    let Some(stored) = ctx
        .pg_loader()
        .load_one(CoinMetadataKey(coin_type))
        .await
        .context("Failed to load info for CoinMetadata")?
    else {
        return Ok(None);
    };

    let id = ObjectID::from_bytes(&stored.object_id).context("Failed to parse ObjectID")?;

    let Some(object) = load_live(ctx, id)
        .await
        .context("Failed to load latest version of CoinMetadata")?
    else {
        return Ok(None);
    };

    let coin_metadata = object
        .try_into()
        .context("Failed to parse object as CoinMetadata")?;

    Ok(Some(coin_metadata))
}

async fn object_with_coin_data(
    ctx: &Context,
    id: ObjectID,
) -> Result<(Object, String, u64), RpcError<Error>> {
    let object = load_live(ctx, id)
        .await?
        .with_context(|| format!("Failed to load latest object {id}"))?;

    let coin = object
        .as_coin_maybe()
        .context("Object is expected to be a coin")?;
    let coin_type = object
        .coin_type_maybe()
        .context("Object is expected to have a coin type")?
        .to_canonical_string(/* with_prefix */ true);
    Ok((object, coin_type, coin.balance.value()))
}
