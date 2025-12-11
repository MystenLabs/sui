// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::str::FromStr;

use anyhow::Context as _;
use futures::future;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use move_core_types::language_storage::{StructTag, TypeTag};
use sui_indexer_alt_reader::consistent_reader::proto::owner::OwnerKind;
use sui_json_rpc_types::{Balance, Coin, Page as PageResponse, SuiCoinMetadata};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::coin::{
    COIN_METADATA_STRUCT_NAME, COIN_MODULE_NAME, COIN_STRUCT_NAME, CoinMetadata,
};
use sui_types::coin_registry::Currency;
use sui_types::object::Object;
use sui_types::{SUI_FRAMEWORK_ADDRESS, parse_sui_type_tag};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    gas_coin::GAS,
};

use crate::{
    context::Context,
    data::load_live,
    error::{InternalContext, RpcError, invalid_params},
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

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Pagination issue: {0}")]
    Pagination(#[from] crate::paginate::Error),

    #[error("Failed to parse type {0:?}: {1}")]
    BadType(String, anyhow::Error),
}

type Cursor = BcsCursor<Vec<u8>>;

#[async_trait::async_trait]
impl CoinsApiServer for Coins {
    async fn get_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: Option<String>,
        limit: Option<usize>,
    ) -> RpcResult<PageResponse<Coin, String>> {
        let inner = if let Some(coin_type) = coin_type {
            parse_sui_type_tag(&coin_type)
                .map_err(|e| invalid_params(Error::BadType(coin_type, e)))?
        } else {
            GAS::type_tag()
        };

        let object_type = StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: COIN_MODULE_NAME.to_owned(),
            name: COIN_STRUCT_NAME.to_owned(),
            type_params: vec![inner],
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

        let consistent_reader = ctx.consistent_reader();

        let results = consistent_reader
            .list_owned_objects(
                None, /* checkpoint */
                OwnerKind::Address,
                Some(owner.to_string()),
                Some(object_type.to_canonical_string(/* with_prefix */ true)),
                Some(page.limit as u32),
                page.cursor.as_ref().map(|c| c.0.clone()),
                None,
                true,
            )
            .await
            .context("Failed to list owned coin objects")
            .map_err(RpcError::<Error>::from)?;

        let coin_ids = results
            .results
            .iter()
            .map(|obj_ref| obj_ref.value.0)
            .collect::<Vec<_>>();

        let next_cursor = results
            .results
            .last()
            .map(|edge| BcsCursor(edge.token.clone()).encode())
            .transpose()
            .context("Failed to encode cursor")
            .map_err(RpcError::<Error>::from)?;

        let coin_futures = coin_ids.iter().map(|id| coin_response(ctx, *id));

        let coins = future::join_all(coin_futures)
            .await
            .into_iter()
            .zip(coin_ids)
            .map(|(r, id)| r.with_internal_context(|| format!("Failed to get object {id}")))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PageResponse {
            data: coins,
            next_cursor,
            has_next_page: results.has_next_page,
        })
    }

    async fn get_coin_metadata(&self, coin_type: String) -> RpcResult<Option<SuiCoinMetadata>> {
        let Self(ctx) = self;

        if let Some(currency) = coin_registry_response(ctx, &coin_type)
            .await
            .with_internal_context(|| format!("Failed to fetch Currency for {coin_type:?}"))?
        {
            return Ok(Some(currency));
        }

        if let Some(metadata) = coin_metadata_response(ctx, &coin_type)
            .await
            .with_internal_context(|| format!("Failed to fetch CoinMetadata for {coin_type:?}"))?
        {
            return Ok(Some(metadata));
        }

        Ok(None)
    }

    async fn get_all_balances(&self, owner: SuiAddress) -> RpcResult<Vec<Balance>> {
        let Self(ctx) = self;
        let consistent_reader = ctx.consistent_reader();
        let config = &ctx.config().coins;

        let mut all_balances = Vec::new();
        let mut after_token: Option<Vec<u8>> = None;

        loop {
            let page = consistent_reader
                .list_balances(
                    None,
                    owner.to_string(),
                    Some(config.max_page_size as u32),
                    after_token.clone(),
                    None,
                    true,
                )
                .await
                .context("Failed to get all balances")
                .map_err(RpcError::<Error>::from)?;

            for edge in &page.results {
                all_balances.push(Balance {
                    coin_type: edge.value.0.to_canonical_string(/* with_prefix */ true),
                    total_balance: edge.value.1 as u128,
                    coin_object_count: 1,
                    locked_balance: HashMap::new(),
                });
            }

            if page.has_next_page {
                after_token = page.results.last().map(|edge| edge.token.clone());
            } else {
                break;
            }
        }

        Ok(all_balances)
    }

    async fn get_balance(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> RpcResult<Balance> {
        let Self(ctx) = self;
        let consistent_reader = ctx.consistent_reader();

        let inner_coin_type = if let Some(coin_type) = coin_type {
            parse_sui_type_tag(&coin_type)
                .map_err(|e| invalid_params(Error::BadType(coin_type, e)))?
        } else {
            GAS::type_tag()
        };

        let (type_tag, total_balance) = consistent_reader
            .get_balance(
                None,
                owner.to_string(),
                inner_coin_type.to_canonical_string(/* with_prefix */ true),
            )
            .await
            .context("Failed to get balance")
            .map_err(RpcError::<Error>::from)?;

        Ok(Balance {
            coin_type: type_tag.to_canonical_string(/* with_prefix */ true),
            total_balance: total_balance as u128,
            coin_object_count: 0,
            locked_balance: HashMap::new(),
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

async fn coin_registry_response(
    ctx: &Context,
    coin_type: &str,
) -> Result<Option<SuiCoinMetadata>, RpcError<Error>> {
    let coin_type = TypeTag::from_str(coin_type)
        .map_err(|e| invalid_params(Error::BadType(coin_type.to_owned(), e)))?;

    let currency_id = Currency::derive_object_id(coin_type)
        .context("Failed to derive object id for coin registry Currency")?;

    let Some(object) = load_live(ctx, currency_id)
        .await
        .context("Failed to load Currency object")?
    else {
        return Ok(None);
    };

    let Some(move_object) = object.data.try_as_move() else {
        return Ok(None);
    };

    let currency: Currency =
        bcs::from_bytes(move_object.contents()).context("Failed to parse Currency object")?;

    Ok(Some(currency.into()))
}

/// Given the inner coin type, i.e 0x2::sui::SUI, load the CoinMetadata object.
async fn coin_metadata_response(
    ctx: &Context,
    coin_type: &str,
) -> Result<Option<SuiCoinMetadata>, RpcError<Error>> {
    let inner = parse_sui_type_tag(coin_type)
        .map_err(|e| invalid_params(Error::BadType(coin_type.to_owned(), e)))?;

    let object_type = StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: COIN_MODULE_NAME.to_owned(),
        name: COIN_METADATA_STRUCT_NAME.to_owned(),
        type_params: vec![inner],
    };

    let Some(obj_ref) = ctx
        .consistent_reader()
        .list_objects_by_type(
            None,
            object_type.to_canonical_string(/* with_prefix */ true),
            Some(1),
            None,
            None,
            false,
        )
        .await
        .context("Failed to load object reference for CoinMetadata")?
        .results
        .into_iter()
        .next()
    else {
        return Ok(None);
    };

    let id = obj_ref.value.0;

    let Some(object) = load_live(ctx, id)
        .await
        .context("Failed to load latest version of CoinMetadata")?
    else {
        return Ok(None);
    };

    let Some(move_object) = object.data.try_as_move() else {
        return Ok(None);
    };

    let coin_metadata: CoinMetadata =
        bcs::from_bytes(move_object.contents()).context("Failed to parse Currency object")?;

    Ok(Some(coin_metadata.into()))
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
