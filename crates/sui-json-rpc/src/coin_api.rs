// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_core_types::language_storage::{StructTag, TypeTag};
use tracing::debug;

use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{Balance, Coin as SuiCoin};
use sui_json_rpc_types::{CoinPage, SuiCoinMetadata};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, ObjectRef, ObjectType, SuiAddress};
use sui_types::coin::{Coin, CoinMetadata};
use sui_types::event::Event;
use sui_types::gas_coin::GAS;
use sui_types::object::{Object, Owner};

use crate::api::{cap_page_limit, CoinReadApiServer};
use crate::error::Error;
use crate::SuiRpcModule;

pub struct CoinReadApi {
    state: Arc<AuthorityState>,
}

impl CoinReadApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }

    async fn get_object(&self, object_id: &ObjectID) -> Result<Object, Error> {
        Ok(self.state.get_object_read(object_id).await?.into_object()?)
    }

    async fn get_coin(&self, coin_id: &ObjectID) -> Result<(StructTag, ObjectRef, Coin), Error> {
        let o = self.get_object(coin_id).await?;
        if let Some(move_object) = o.data.try_as_move() {
            Ok((
                move_object.type_.clone(),
                o.compute_object_reference(),
                bcs::from_bytes(move_object.contents())?,
            ))
        } else {
            Err(Error::UnexpectedError(format!(
                "Provided object : [{coin_id}] is not a Move object."
            )))
        }
    }

    fn get_owner_coin_iterator<'a>(
        &'a self,
        owner: SuiAddress,
        coin_type: &'a Option<String>,
    ) -> Result<impl Iterator<Item = ObjectID> + '_, Error> {
        Ok(self
            .state
            .get_owner_objects_iterator(Owner::AddressOwner(owner))?
            .filter(move |o| matches!(&o.type_, ObjectType::Struct(type_) if is_coin_type(type_, &coin_type)))
            .map(|info|info.object_id))
    }
}

impl SuiRpcModule for CoinReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::CoinReadApiOpenRpc::module_doc()
    }
}

#[async_trait]
impl CoinReadApiServer for CoinReadApi {
    async fn get_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<CoinPage> {
        // TODO: Add index to improve performance?
        let limit = cap_page_limit(limit)?;
        let mut coins = self
            .get_owner_coin_iterator(owner, &coin_type)?
            .skip_while(|o| matches!(&cursor, Some(cursor) if cursor != o))
            .take(limit + 1)
            .collect::<Vec<_>>();

        let next_cursor = coins.get(limit).cloned();
        coins.truncate(limit);

        let mut data = vec![];

        for coin in coins {
            let (type_, oref, coin) = self.get_coin(&coin).await?;
            // We have checked these are coin objects, safe to unwrap.
            let coin_type = type_.type_params.first().unwrap().to_string();
            data.push(SuiCoin {
                coin_type,
                coin_object_id: oref.0,
                version: oref.1,
                digest: oref.2,
                balance: coin.balance.value(),
            })
        }
        Ok(CoinPage { data, next_cursor })
    }

    async fn get_balances(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> RpcResult<Vec<Balance>> {
        // TODO: Add index to improve performance?
        let coins = self.get_owner_coin_iterator(owner, &coin_type)?;
        let mut data: HashMap<String, (u128, usize)> = HashMap::new();

        for coin in coins {
            let (type_, _, coin) = self.get_coin(&coin).await?;
            let coin_type = type_.type_params.first().unwrap().to_string();
            let (amount, count) = data.entry(coin_type).or_default();
            *amount += coin.balance.value() as u128;
            *count += 1;
        }

        Ok(data
            .into_iter()
            .map(|(coin_type, (total_balance, coin_object_count))| Balance {
                coin_type,
                coin_object_count,
                total_balance,
            })
            .collect())
    }

    async fn get_coin_metadata(&self, coin_type: String) -> RpcResult<SuiCoinMetadata> {
        let coin_struct = coin_type.parse::<StructTag>().map_err(|e| anyhow!("{e}"))?;
        if GAS::is_gas(&coin_struct) {
            // TODO: We need to special case for `CoinMetadata<0x2::sui::SUI> because `get_transaction`
            // will fail for genesis transaction. However, instead of hardcoding the values here, We
            // can store the object id for `CoinMetadata<0x2::sui::SUI>` in the Sui System object
            return Ok(SuiCoinMetadata {
                id: None,
                decimals: 9,
                symbol: "SUI".to_string(),
                name: "Sui".to_string(),
                description: "".to_string(),
                icon_url: None,
            });
        }
        let publish_txn_digest = self
            .get_object(&coin_struct.address.into())
            .await?
            .previous_transaction;
        let (_, effects) = self.state.get_transaction(publish_txn_digest).await?;
        let event = effects
            .events
            .into_iter()
            .find(|e| {
                if let Event::NewObject { object_type, .. } = e {
                    return object_type.parse::<StructTag>().map_or(false, |tag| {
                        CoinMetadata::is_coin_metadata(&tag)
                            && tag.type_params.len() == 1
                            && tag.type_params[0].to_canonical_string()
                                == coin_struct.to_canonical_string()
                    });
                }
                false
            })
            .ok_or(0)
            .map_err(|_| anyhow!("No NewObject event was emitted for CoinMetaData"))?;

        let metadata_object_id = event
            .object_id()
            .ok_or(0)
            .map_err(|_| anyhow!("No object id is found in NewObject event"))?;

        Ok(self
            .state
            .get_object_read(&metadata_object_id)
            .await
            .map_err(|e| {
                debug!(?metadata_object_id, "Failed to get object: {:?}", e);
                anyhow!("{e}")
            })?
            .into_object()
            .map_err(|e| {
                debug!(
                    ?metadata_object_id,
                    "Failed to convert ObjectRead to Object: {:?}", e
                );
                anyhow!("{e}")
            })?
            .try_into()
            .map_err(|e| {
                debug!(
                    ?metadata_object_id,
                    "Failed to convert object to CoinMetadata: {:?}", e
                );
                anyhow!("{e}")
            })?)
    }
}

fn is_coin_type(type_: &StructTag, coin_type: &Option<String>) -> bool {
    if Coin::is_coin(type_) {
        return if let Some(coin_type) = coin_type {
            matches!(type_.type_params.first(), Some(TypeTag::Struct(type_)) if &type_.to_string() == coin_type)
        } else {
            true
        };
    }
    false
}
