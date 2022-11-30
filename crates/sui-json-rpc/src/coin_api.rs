// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use move_core_types::language_storage::StructTag;
use tracing::debug;

use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{BalancePage, CoinPage, SuiCoinMetadata};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::coin::CoinMetadata;
use sui_types::event::Event;
use sui_types::gas_coin::GAS;

use crate::api::CoinReadApiServer;

pub struct CoinReadApi {
    pub state: Arc<AuthorityState>,
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
        todo!()
    }

    async fn get_balances(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: Option<String>,
        limit: Option<usize>,
    ) -> RpcResult<BalancePage> {
        todo!()
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
            .state
            .get_object_read(&coin_struct.address.into())
            .await
            .map_err(|e| anyhow!("{e}"))?
            .into_object()
            .map_err(|e| anyhow!("{e}"))?
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
