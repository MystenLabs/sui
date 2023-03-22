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
use sui_types::balance::Supply;
use sui_types::base_types::{MoveObjectType, ObjectID, ObjectRef, ObjectType, SuiAddress};
use sui_types::coin::{Coin, CoinMetadata, LockedCoin, TreasuryCap};
use sui_types::error::SuiError;
use sui_types::gas_coin::GAS;
use sui_types::messages::TransactionEffectsAPI;
use sui_types::object::{Object, Owner};
use sui_types::parse_sui_struct_tag;

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
        Ok(self
            .state
            .get_object_read(object_id)
            .await?
            .into_object()
            .map_err(SuiError::from)?)
    }

    async fn get_coin(&self, coin_id: &ObjectID) -> Result<SuiCoin, Error> {
        let o = self.get_object(coin_id).await?;
        if let Some(move_object) = o.data.try_as_move() {
            let (balance, locked_until_epoch) = if move_object.type_().is_coin() {
                let coin: Coin = bcs::from_bytes(move_object.contents())?;
                (coin.balance.value(), None)
            } else if move_object.type_().is_locked_coin() {
                let locked_coin: LockedCoin = bcs::from_bytes(move_object.contents())?;
                (
                    locked_coin.balance.value(),
                    Some(locked_coin.locked_until_epoch),
                )
            } else {
                return Err(Error::SuiError(SuiError::ObjectDeserializationError {
                    error: format!("{:?} is not a supported coin type", move_object.type_()),
                }));
            };

            Ok(SuiCoin {
                coin_type: move_object
                    .type_()
                    .type_params()
                    .first()
                    .unwrap()
                    .to_string(),
                coin_object_id: o.id(),
                version: o.version(),
                digest: o.digest(),
                balance,
                locked_until_epoch,
                previous_transaction: o.previous_transaction,
            })
        } else {
            Err(Error::UnexpectedError(format!(
                "Provided object : [{coin_id}] is not a Move object."
            )))
        }
    }

    async fn get_coins_internal(
        &self,
        owner: SuiAddress,
        coin_type: Option<StructTag>,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> Result<CoinPage, Error> {
        // TODO: Add index to improve performance?
        let limit = cap_page_limit(limit);
        let mut coins = self
            .get_owner_coin_iterator(owner, &coin_type)?
            .skip_while(|o| matches!(&cursor, Some(cursor) if cursor != o))
            // skip an extra b/c the cursor is exclusive
            .skip(usize::from(cursor.is_some()))
            .take(limit + 1)
            .collect::<Vec<_>>();

        let has_next_page = coins.len() > limit;
        coins.truncate(limit);
        let next_cursor = coins.last().cloned().map_or(cursor, Some);

        let mut data = vec![];
        for coin in coins {
            data.push(self.get_coin(&coin).await?)
        }
        Ok(CoinPage {
            data,
            next_cursor,
            has_next_page,
        })
    }

    fn get_owner_coin_iterator<'a>(
        &'a self,
        owner: SuiAddress,
        coin_type: &'a Option<StructTag>,
    ) -> Result<impl Iterator<Item = ObjectID> + '_, Error> {
        Ok(self
            .state
            .get_owner_objects_iterator(owner, None, None, None)?
            .filter(move |o| matches!(&o.type_, ObjectType::Struct(type_) if is_coin_type(type_, coin_type)))
            .map(|info|info.object_id))
    }

    async fn find_package_object(
        &self,
        package_id: &ObjectID,
        object_struct_tag: StructTag,
    ) -> Result<Object, Error> {
        let publish_txn_digest = self.get_object(package_id).await?.previous_transaction;
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
        self.get_object(&object_id).await
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
        let coin_type = Some(match coin_type {
            Some(c) => parse_sui_struct_tag(&c)?,
            None => GAS::type_(),
        });
        Ok(self
            .get_coins_internal(owner, coin_type, cursor, limit)
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

    async fn get_balance(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> RpcResult<Balance> {
        let coin_type = Some(match coin_type {
            Some(c) => parse_sui_struct_tag(&c)?,
            None => GAS::type_(),
        });

        // TODO: Add index to improve performance?
        let coins = self.get_owner_coin_iterator(owner, &coin_type)?;
        let mut total_balance = 0u128;
        let mut locked_balance = HashMap::new();
        let mut coin_object_count = 0;

        for coin in coins {
            let coin = self.get_coin(&coin).await?;
            if let Some(lock) = coin.locked_until_epoch {
                *locked_balance.entry(lock).or_default() += coin.balance as u128
            } else {
                total_balance += coin.balance as u128;
            }
            coin_object_count += 1;
        }

        Ok(Balance {
            coin_type: coin_type.unwrap().to_string(),
            coin_object_count,
            total_balance,
            locked_balance,
        })
    }

    async fn get_all_balances(&self, owner: SuiAddress) -> RpcResult<Vec<Balance>> {
        // TODO: Add index to improve performance?
        let coins = self.get_owner_coin_iterator(owner, &None)?;
        let mut balances: HashMap<String, Balance> = HashMap::new();

        for coin in coins {
            let coin = self.get_coin(&coin).await?;
            let balance = balances.entry(coin.coin_type.clone()).or_insert(Balance {
                coin_type: coin.coin_type,
                coin_object_count: 0,
                total_balance: 0,
                locked_balance: Default::default(),
            });
            if let Some(lock) = coin.locked_until_epoch {
                *balance.locked_balance.entry(lock).or_default() += coin.balance as u128
            } else {
                balance.total_balance += coin.balance as u128;
            }
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

fn is_coin_type(type_: &MoveObjectType, coin_type: &Option<StructTag>) -> bool {
    if type_.is_coin() || type_.is_locked_coin() {
        return if let Some(coin_type) = coin_type {
            matches!(type_.type_params().first(), Some(TypeTag::Struct(type_)) if type_.to_canonical_string() == coin_type.to_canonical_string())
        } else {
            true
        };
    }
    false
}
