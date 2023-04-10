// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_core_types::language_storage::{StructTag, TypeTag};
use tracing::debug;

use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{Balance, Coin as SuiCoin};
use sui_json_rpc_types::{CoinPage, SuiCoinMetadata};
use sui_open_rpc::Module;
use sui_types::balance::Supply;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::coin::{Coin, CoinMetadata, TreasuryCap};
use sui_types::error::{SuiError, UserInputError};
use sui_types::gas_coin::GAS;
use sui_types::messages::TransactionEffectsAPI;
use sui_types::object::{Object, Owner};
use sui_types::parse_sui_struct_tag;
use sui_types::storage::ObjectKey;

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

    fn multi_get_coin_objects(&self, coins: &[ObjectRef]) -> Result<Vec<Object>, Error> {
        Ok(self
            .state
            .database
            .multi_get_object_by_key(&coins.iter().map(ObjectKey::from).collect::<Vec<_>>())?
            .into_iter()
            .zip(coins)
            .map(|(o, (id, version, _digest))| {
                o.ok_or(UserInputError::ObjectNotFound {
                    object_id: *id,
                    version: Some(*version),
                })
            })
            .collect::<Result<Vec<_>, UserInputError>>()?)
    }

    /// Fetch all of the objects in `coins`. It's the caller's responsibility
    /// to ensure that every ObjRef in `coins` is in fact a coin by using `Authority::get_owner_coin_iterator`,
    /// and that every coin is of type `coin_type_tag`.
    /// Note: if  we are fetching gas coins, `coin_type_tag` should be `Some(SUI)`, not `Some(Coin<SUI>)`
    fn multi_get_coin(
        &self,
        coins: &[ObjectRef],
        coin_type_tag: Option<&TypeTag>,
    ) -> Result<Vec<Result<SuiCoin, Error>>, Error> {
        let o = self
            .state
            .database
            .multi_get_object_by_key(&coins.iter().map(ObjectKey::from).collect::<Vec<_>>())?;
        // conversion from TypeTag to string is expensive, so do it outside the loop if we already know the coin type
        // if coin_type_tag is None, we are getting a heterogenous mix of coins and we have no choice but to string-ify in the loop
        let coin_type_str = coin_type_tag.map(|t| t.to_string());

        Ok(o.into_iter()
            .zip(coins)
            .map(|(o, (id, version, digest))| {
                let o = o.ok_or(UserInputError::ObjectNotFound {
                    object_id: *id,
                    version: Some(*version),
                })?;

                if let Some(move_object) = o.data.try_as_move() {
                    let coin_type = coin_type_str.clone().unwrap_or_else(|| {
                        o.type_()
                            .unwrap()
                            .type_params()
                            .first()
                            .unwrap()
                            .to_string()
                    });
                    let balance = {
                        let coin: Coin = bcs::from_bytes(move_object.contents())?;
                        coin.balance.value()
                    };

                    Ok(SuiCoin {
                        coin_type,
                        coin_object_id: *id,
                        version: *version,
                        digest: *digest,
                        balance,
                        locked_until_epoch: None,
                        previous_transaction: o.previous_transaction,
                    })
                } else {
                    Err(Error::UnexpectedError(format!(
                        "Provided object : [{}] is not a Move object.",
                        o.id()
                    )))
                }
            })
            .collect())
    }

    async fn get_coins_internal(
        &self,
        owner: SuiAddress,
        coin_type: Option<&TypeTag>,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> Result<CoinPage, Error> {
        // TODO: Add index to improve performance?
        let limit = cap_page_limit(limit);
        let mut coins = self
            .get_owner_coin_iterator(owner, coin_type)?
            .skip_while(|o| matches!(&cursor, Some(cursor) if cursor != &o.0))
            // skip an extra b/c the cursor is exclusive
            .skip(usize::from(cursor.is_some()))
            .take(limit + 1)
            .collect::<Vec<_>>();

        let has_next_page = coins.len() > limit;
        coins.truncate(limit);
        let next_cursor = coins.last().cloned().map_or(cursor, |(id, _, _)| Some(id));

        let data = self
            .multi_get_coin(&coins, coin_type)?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CoinPage {
            data,
            next_cursor,
            has_next_page,
        })
    }

    fn get_owner_coin_iterator<'a>(
        &'a self,
        owner: SuiAddress,
        coin_type: Option<&'a TypeTag>,
    ) -> Result<impl Iterator<Item = ObjectRef> + '_, Error> {
        Ok(self
            .state
            .get_owner_objects_iterator(owner, None, None)?
            .filter(move |o| {
                if let Some(coin_type) = coin_type {
                    o.type_.is_coin_t(coin_type)
                } else {
                    o.type_.is_coin()
                }
            })
            .map(|info| (info.object_id, info.version, info.digest)))
    }

    async fn find_package_object(
        &self,
        package_id: &ObjectID,
        object_struct_tag: StructTag,
    ) -> Result<Object, Error> {
        let publish_txn_digest = self
            .state
            .get_object_read(package_id)?
            .into_object()?
            .previous_transaction;
        let (_, effect) = self
            .state
            .get_executed_transaction_and_effects(publish_txn_digest)
            .await?;
        let created: &[(ObjectRef, Owner)] = effect.created();

        let object_id = async {
            for ((id, version, _), _) in created {
                if let Ok(past_object) = self.state.get_past_object_read(id, *version).await {
                    if let Ok(object) = past_object.into_object() {
                        if matches!(object.type_(), Some(type_) if type_.is(&object_struct_tag)) {
                            return Ok(*id);
                        }
                    }
                }
            }
            Err(anyhow!(
                "Cannot find object [{}] from [{}] package event.",
                object_struct_tag,
                package_id
            ))
        }
        .await?;
        Ok(self.state.get_object_read(&object_id)?.into_object()?)
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
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<CoinPage> {
        let coin_type = TypeTag::Struct(Box::new(match coin_type {
            Some(c) => parse_sui_struct_tag(&c)?,
            None => GAS::type_(),
        }));
        Ok(self
            .get_coins_internal(owner, Some(&coin_type), cursor, limit)
            .await?)
    }

    async fn get_all_coins(
        &self,
        owner: SuiAddress,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<CoinPage> {
        Ok(self.get_coins_internal(owner, None, cursor, limit).await?)
    }

    fn get_balance(&self, owner: SuiAddress, coin_type: Option<String>) -> RpcResult<Balance> {
        let coin_type = TypeTag::Struct(Box::new(match coin_type {
            Some(c) => parse_sui_struct_tag(&c)?,
            None => GAS::type_(),
        }));

        // TODO: Add index to improve performance?
        let coins = self.multi_get_coin_objects(
            &self
                .get_owner_coin_iterator(owner, Some(&coin_type))?
                .collect::<Vec<_>>(),
        )?;
        let mut total_balance = 0u128;
        let mut coin_object_count = 0;

        for coin_obj in coins {
            // unwraps safe because get_owner_coin_iterator can only return coin objects
            let coin: Coin =
                bcs::from_bytes(coin_obj.data.try_as_move().unwrap().contents()).unwrap();
            total_balance += coin.balance.value() as u128;
            coin_object_count += 1;
        }

        Ok(Balance {
            coin_type: coin_type.to_string(),
            coin_object_count,
            total_balance,
            // note: LockedCoin is deprecated
            locked_balance: Default::default(),
        })
    }

    fn get_all_balances(&self, owner: SuiAddress) -> RpcResult<Vec<Balance>> {
        let mut balances: HashMap<TypeTag, Balance> = HashMap::new();
        // TODO: Add index to improve performance?
        let coin_objs = self.multi_get_coin_objects(
            &self
                .get_owner_coin_iterator(owner, None)?
                .collect::<Vec<_>>(),
        )?;
        for coin_obj in coin_objs {
            // unwrap safe because get_owner_coin_iterator can only return coin objects
            let move_obj = coin_obj.data.try_as_move().unwrap();
            // unwrap safe because each coin object has one type param
            let coin_type = move_obj.type_().type_params().first().unwrap().clone();
            let coin: Coin = bcs::from_bytes(move_obj.contents()).unwrap();

            let balance = balances.entry(coin_type.clone()).or_insert(Balance {
                coin_type: coin_type.to_string(),
                coin_object_count: 0,
                total_balance: 0,
                // note: LockedCoin is deprecated
                locked_balance: Default::default(),
            });
            balance.total_balance += coin.balance.value() as u128;
            balance.coin_object_count += 1;
        }
        Ok(balances.into_values().collect())
    }

    async fn get_coin_metadata(&self, coin_type: String) -> RpcResult<SuiCoinMetadata> {
        let coin_struct = parse_sui_struct_tag(&coin_type)?;
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

        let metadata_object = self
            .find_package_object(
                &coin_struct.address.into(),
                CoinMetadata::type_(coin_struct),
            )
            .await?;
        let metadata_object_id = metadata_object.id();
        Ok(metadata_object.try_into().map_err(|e: SuiError| {
            debug!(
                ?metadata_object_id,
                "Failed to convert object to CoinMetadata: {:?}", e
            );
            Error::from(e)
        })?)
    }

    async fn get_total_supply(&self, coin_type: String) -> RpcResult<Supply> {
        let coin_struct = parse_sui_struct_tag(&coin_type)?;

        Ok(if GAS::is_gas(&coin_struct) {
            Supply { value: 0 }
        } else {
            let treasury_cap_object = self
                .find_package_object(&coin_struct.address.into(), TreasuryCap::type_(coin_struct))
                .await?;

            let treasury_cap = TreasuryCap::from_bcs_bytes(
                treasury_cap_object.data.try_as_move().unwrap().contents(),
            )
            .map_err(Error::from)?;
            treasury_cap.total_supply
        })
    }
}
