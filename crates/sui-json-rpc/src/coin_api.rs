// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use cached::proc_macro::cached;
use cached::SizedCache;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_core_types::language_storage::{StructTag, TypeTag};
use sui_storage::indexes::TotalBalance;
use sui_types::digests::TransactionDigest;
use sui_types::transaction::VerifiedTransaction;
use tracing::{debug, info, instrument};

use mysten_metrics::spawn_monitored_task;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{Balance, Coin as SuiCoin};
use sui_json_rpc_types::{CoinPage, SuiCoinMetadata};
use sui_open_rpc::Module;
use sui_types::balance::Supply;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::coin::{CoinMetadata, TreasuryCap};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::error::{SuiError, SuiResult};
use sui_types::gas_coin::GAS;
use sui_types::object::{Object, ObjectRead};
use sui_types::parse_sui_struct_tag;

use crate::api::{cap_page_limit, CoinReadApiServer, JsonRpcMetrics};
use crate::error::{Error, SuiApiResult, SuiRpcInputError};
use crate::{with_tracing, SuiRpcModule};

#[cfg(test)]
use mockall::automock;

pub struct CoinReadApi {
    // Trait object w/ Box as we do not need to share this across multiple threads
    internal: Box<dyn CoinReadInternal + Send + Sync>,
}

impl CoinReadApi {
    pub fn new(state: Arc<AuthorityState>, metrics: Arc<JsonRpcMetrics>) -> Self {
        Self {
            internal: Box::new(CoinReadInternalImpl::new(state, metrics)),
        }
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
    #[instrument(skip(self))]
    async fn get_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<CoinPage> {
        with_tracing!(async move {
            let coin_type_tag = TypeTag::Struct(Box::new(match coin_type {
                Some(c) => parse_sui_struct_tag(&c).map_err(|e| {
                    Error::SuiRpcInputError(
                        // todo: clean up parse_sui_struct_tag to return actionable errors
                        SuiRpcInputError::CannotParseSuiStructTag(format!("{e}")),
                    )
                })?,
                None => GAS::type_(),
            }));

            let cursor = match cursor {
                Some(c) => (coin_type_tag.to_string(), c),
                // If cursor is not specified, we need to start from the beginning of the coin type, which is the minimal possible ObjectID.
                None => (coin_type_tag.to_string(), ObjectID::ZERO),
            };

            let coins = self
                .internal
                .get_coins_iterator(
                    owner, cursor, limit, true, // only care about one type of coin
                )
                .await?;

            Ok(coins)
        })
    }

    #[instrument(skip(self))]
    async fn get_all_coins(
        &self,
        owner: SuiAddress,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<CoinPage> {
        with_tracing!(async move {
            let cursor = match cursor {
                Some(object_id) => {
                    let obj = self
                        .internal
                        .get_state()
                        .get_object(&object_id)
                        .await
                        .map_err(Error::from)?;
                    match obj {
                        Some(obj) => {
                            let coin_type = obj.coin_type_maybe();
                            if coin_type.is_none() {
                                Err(Error::SuiRpcInputError(SuiRpcInputError::GenericInvalid(
                                    format!("Invalid Cursor {:?}, Object is not a coin", object_id),
                                )))
                            } else {
                                Ok((coin_type.unwrap().to_string(), object_id))
                            }
                        }
                        None => Err(Error::SuiRpcInputError(SuiRpcInputError::GenericInvalid(
                            format!("Invalid Cursor {:?}, Object not found", object_id),
                        ))),
                    }
                }
                None => {
                    // If cursor is None, start from the beginning
                    Ok((String::from_utf8([0u8].to_vec()).unwrap(), ObjectID::ZERO))
                }
            }?;

            let coins = self
                .internal
                .get_coins_iterator(
                    owner, cursor, limit, false, // return all types of coins
                )
                .await?;

            Ok(coins)
        })
    }

    #[instrument(skip(self))]
    async fn get_balance(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> RpcResult<Balance> {
        with_tracing!(async move {
            let coin_type = TypeTag::Struct(Box::new(match coin_type {
                Some(c) => parse_sui_struct_tag(&c).map_err(|_| {
                    Error::SuiRpcInputError(SuiRpcInputError::CannotParseSuiStructTag(c))
                })?,
                None => GAS::type_(),
            }));
            let balance = self
                .internal
                .get_state()
                .get_balance(owner, coin_type.clone())
                .await
                .map_err(|e: SuiError| {
                    debug!(?owner, "Failed to get balance with error: {:?}", e);
                    Error::from(e)
                })?;
            Ok(Balance {
                coin_type: coin_type.to_string(),
                coin_object_count: balance.num_coins as usize,
                total_balance: balance.balance as u128,
                // note: LockedCoin is deprecated
                locked_balance: Default::default(),
            })
        })
    }

    #[instrument(skip(self))]
    async fn get_all_balances(&self, owner: SuiAddress) -> RpcResult<Vec<Balance>> {
        with_tracing!(async move {
            let all_balance = self
                .internal
                .get_state()
                .get_all_balance(owner)
                .await
                .map_err(|e: SuiError| {
                    debug!(?owner, "Failed to get all balance with error: {:?}", e);
                    Error::from(e)
                })?;
            Ok(all_balance
                .iter()
                .map(|(coin_type, balance)| {
                    Balance {
                        coin_type: coin_type.to_string(),
                        coin_object_count: balance.num_coins as usize,
                        total_balance: balance.balance as u128,
                        // note: LockedCoin is deprecated
                        locked_balance: Default::default(),
                    }
                })
                .collect())
        })
    }

    #[instrument(skip(self))]
    async fn get_coin_metadata(&self, coin_type: String) -> RpcResult<Option<SuiCoinMetadata>> {
        with_tracing!(async move {
            let coin_struct = parse_sui_struct_tag(&coin_type).map_err(|_| {
                Error::SuiRpcInputError(SuiRpcInputError::CannotParseSuiStructTag(coin_type))
            })?;
            Ok(self.internal.get_coin_metadata(coin_struct).await?)
        })
    }

    #[instrument(skip(self))]
    async fn get_total_supply(&self, coin_type: String) -> RpcResult<Supply> {
        with_tracing!(async move {
            let coin_struct = parse_sui_struct_tag(&coin_type).map_err(|_| {
                Error::SuiRpcInputError(SuiRpcInputError::CannotParseSuiStructTag(coin_type))
            })?;

            Ok(if GAS::is_gas(&coin_struct) {
                Supply { value: 0 }
            } else {
                let treasury_cap = self.internal.get_treasury_cap(coin_struct).await?;
                treasury_cap.total_supply
            })
        })
    }
}

/// State trait to capture subset of AuthorityState used by CoinReadApi
/// This allows us to also mock AuthorityState for testing
#[cfg_attr(test, automock)]
#[async_trait]
pub trait State {
    fn get_object_read(&self, object_id: &ObjectID) -> SuiResult<ObjectRead>;
    async fn get_object(&self, object_id: &ObjectID) -> SuiResult<Option<Object>>;
    fn find_publish_txn_digest(&self, package_id: ObjectID) -> SuiResult<TransactionDigest>;
    async fn find_package_object(
        &self,
        package_id: &ObjectID,
        object_struct_tag: StructTag,
    ) -> SuiApiResult<Object>;
    fn get_owned_coins(
        &self,
        owner: SuiAddress,
        cursor: (String, ObjectID),
        limit: usize,
        one_coin_type_only: bool,
    ) -> SuiResult<Vec<SuiCoin>>;
    async fn get_executed_transaction_and_effects(
        &self,
        digest: TransactionDigest,
    ) -> SuiResult<(VerifiedTransaction, TransactionEffects)>;
    async fn get_balance(&self, owner: SuiAddress, coin_type: TypeTag) -> SuiResult<TotalBalance>;
    async fn get_all_balance(
        &self,
        owner: SuiAddress,
    ) -> SuiResult<Arc<HashMap<TypeTag, TotalBalance>>>;
}

/// We set StateImpl as a wrapper over Arc<AuthorityState> primarily due to the need to pass Arc<AuthorityState> to find_package_object_id
pub struct StateImpl {
    state: Arc<AuthorityState>,
}

impl StateImpl {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl State for StateImpl {
    fn get_object_read(&self, object_id: &ObjectID) -> SuiResult<ObjectRead> {
        self.state.get_object_read(object_id)
    }

    async fn get_object(&self, object_id: &ObjectID) -> SuiResult<Option<Object>> {
        self.state.get_object(object_id).await
    }

    fn find_publish_txn_digest(&self, package_id: ObjectID) -> SuiResult<TransactionDigest> {
        self.state.find_publish_txn_digest(package_id)
    }

    async fn find_package_object(
        &self,
        package_id: &ObjectID,
        object_struct_tag: StructTag,
    ) -> SuiApiResult<Object> {
        let object_id =
            find_package_object_id(self.state.clone(), *package_id, object_struct_tag).await?;
        Ok(self.get_object_read(&object_id)?.into_object()?)
    }

    fn get_owned_coins(
        &self,
        owner: SuiAddress,
        cursor: (String, ObjectID),
        limit: usize,
        one_coin_type_only: bool,
    ) -> SuiResult<Vec<SuiCoin>> {
        Ok(self
            .state
            .get_owned_coins_iterator_with_cursor(owner, cursor, limit, one_coin_type_only)?
            .map(|(coin_type, coin_object_id, coin)| SuiCoin {
                coin_type,
                coin_object_id,
                version: coin.version,
                digest: coin.digest,
                balance: coin.balance,
                previous_transaction: coin.previous_transaction,
            })
            .collect::<Vec<_>>())
    }

    async fn get_executed_transaction_and_effects(
        &self,
        digest: TransactionDigest,
    ) -> SuiResult<(VerifiedTransaction, TransactionEffects)> {
        self.state
            .get_executed_transaction_and_effects(digest)
            .await
    }

    async fn get_balance(&self, owner: SuiAddress, coin_type: TypeTag) -> SuiResult<TotalBalance> {
        self.state
            .indexes
            .as_ref()
            .ok_or(SuiError::IndexStoreNotAvailable)?
            .get_balance(owner, coin_type)
            .await
    }

    async fn get_all_balance(
        &self,
        owner: SuiAddress,
    ) -> SuiResult<Arc<HashMap<TypeTag, TotalBalance>>> {
        self.state
            .indexes
            .as_ref()
            .ok_or(SuiError::IndexStoreNotAvailable)?
            .get_all_balance(owner)
            .await
    }
}

#[cached(
    type = "SizedCache<String, ObjectID>",
    create = "{ SizedCache::with_size(10000) }",
    convert = r#"{ format!("{}{}", package_id, object_struct_tag) }"#,
    result = true
)]
async fn find_package_object_id(
    state: Arc<AuthorityState>,
    package_id: ObjectID,
    object_struct_tag: StructTag,
) -> SuiApiResult<ObjectID> {
    spawn_monitored_task!(async move {
        let publish_txn_digest = state.find_publish_txn_digest(package_id)?;

        let (_, effect) = state
            .get_executed_transaction_and_effects(publish_txn_digest)
            .await?;

        for ((id, _, _), _) in effect.created() {
            if let Ok(object_read) = state.get_object_read(id) {
                if let Ok(object) = object_read.into_object() {
                    if matches!(object.type_(), Some(type_) if type_.is(&object_struct_tag)) {
                        return Ok(*id);
                    }
                }
            }
        }
        Err(SuiRpcInputError::GenericNotFound(format!(
            "Cannot find object [{}] from [{}] package event.",
            object_struct_tag, package_id,
        ))
        .into())
    })
    .await?
}

/// CoinReadInternal trait to capture logic of interactions with AuthorityState and metrics
/// This allows us to also mock internal implementation for testing
#[cfg_attr(test, automock)]
#[async_trait]
pub trait CoinReadInternal {
    fn get_state(&self) -> Arc<dyn State + Send + Sync>;
    async fn get_coins_iterator(
        &self,
        owner: SuiAddress,
        cursor: (String, ObjectID),
        limit: Option<usize>,
        one_coin_type_only: bool,
    ) -> SuiApiResult<CoinPage>;
    async fn get_coin_metadata(
        &self,
        coin_struct: StructTag,
    ) -> SuiApiResult<Option<SuiCoinMetadata>>;
    async fn get_treasury_cap(&self, coin_struct: StructTag) -> SuiApiResult<TreasuryCap>;
}

pub struct CoinReadInternalImpl {
    // Trait object w/ Arc as we have methods that require sharing this across multiple threads
    state: Arc<dyn State + Send + Sync>,
    pub metrics: Arc<JsonRpcMetrics>,
}

impl CoinReadInternalImpl {
    pub fn new(state: Arc<AuthorityState>, metrics: Arc<JsonRpcMetrics>) -> Self {
        Self {
            state: Arc::new(StateImpl::new(state)),
            metrics,
        }
    }
}

#[async_trait]
impl CoinReadInternal for CoinReadInternalImpl {
    fn get_state(&self) -> Arc<dyn State + Send + Sync> {
        self.state.clone()
    }

    async fn get_coins_iterator(
        &self,
        owner: SuiAddress,
        cursor: (String, ObjectID),
        limit: Option<usize>,
        one_coin_type_only: bool,
    ) -> SuiApiResult<CoinPage> {
        let limit = cap_page_limit(limit);
        self.metrics.get_coins_limit.report(limit as u64);

        // This is needed for spawn_monitored_task
        let state = self.get_state();
        let mut data = spawn_monitored_task!(async move {
            state.get_owned_coins(owner, cursor, limit + 1, one_coin_type_only)
        })
        .await??;

        let has_next_page = data.len() > limit;
        data.truncate(limit);

        self.metrics.get_coins_result_size.report(data.len() as u64);
        self.metrics
            .get_coins_result_size_total
            .inc_by(data.len() as u64);
        let next_cursor = data.last().map(|coin| coin.coin_object_id);
        Ok(CoinPage {
            data,
            next_cursor,
            has_next_page,
        })
    }

    async fn get_coin_metadata(
        &self,
        coin_struct: StructTag,
    ) -> SuiApiResult<Option<SuiCoinMetadata>> {
        let metadata_object = self
            .state
            .find_package_object(
                &coin_struct.address.into(),
                CoinMetadata::type_(coin_struct),
            )
            .await
            .ok();
        Ok(metadata_object.and_then(|v: Object| v.try_into().ok()))
    }

    async fn get_treasury_cap(&self, coin_struct: StructTag) -> SuiApiResult<TreasuryCap> {
        let treasury_cap_object = self
            .state
            .find_package_object(&coin_struct.address.into(), TreasuryCap::type_(coin_struct))
            .await?;

        Ok(TreasuryCap::from_bcs_bytes(
            treasury_cap_object.data.try_as_move().unwrap().contents(),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use jsonrpsee::types::ErrorObjectOwned;
    use move_core_types::language_storage::StructTag;
    use sui_types::balance::Supply;
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::coin::TreasuryCap;
    use sui_types::id::UID;
    use sui_types::object::Object;
    use sui_types::parse_sui_struct_tag;

    fn get_test_owner() -> SuiAddress {
        SuiAddress::random_for_testing_only()
    }

    fn get_test_package_id() -> ObjectID {
        ObjectID::random()
    }

    fn get_test_coin_type(package_id: ObjectID) -> String {
        format!("{}::test_coin::TEST_COIN", package_id)
    }

    fn get_test_treasury_cap_peripherals(
        package_id: ObjectID,
    ) -> (String, StructTag, StructTag, TreasuryCap, Object) {
        let coin_name = get_test_coin_type(package_id);
        let input_coin_struct = parse_sui_struct_tag(&coin_name).expect("should not fail");
        let treasury_cap_struct = TreasuryCap::type_(input_coin_struct.clone());
        let treasury_cap = TreasuryCap {
            id: UID::new(ObjectID::random()),
            total_supply: Supply { value: 420 },
        };
        let treasury_cap_object =
            Object::treasury_cap_for_testing(input_coin_struct.clone(), treasury_cap.clone());
        (
            coin_name,
            input_coin_struct,
            treasury_cap_struct,
            treasury_cap,
            treasury_cap_object,
        )
    }

    mod get_coins_tests {
        use super::super::*;
        use super::*;
        use jsonrpsee::types::ErrorObjectOwned;
        use typed_store::TypedStoreError;

        #[tokio::test]
        async fn test_invalid_coin_type() {
            let owner = get_test_owner();
            let coin_type = "0x2::invalid::struct::tag";
            let mock_internal = MockCoinReadInternal::new();
            let coin_read_api = CoinReadApi {
                internal: Box::new(mock_internal),
            };

            let response = coin_read_api
                .get_coins(owner, Some(coin_type.to_string()), None, None)
                .await;

            assert!(response.is_err());
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();
            assert_eq!(error_object.code(), -32602);
            assert_eq!(
                error_object.message(),
                "Invalid struct type: 0x2::invalid::struct::tag. Got error: Expected end of token stream. Got: ::"
            );
        }

        #[tokio::test]
        async fn test_unrecognized_token() {
            let owner = get_test_owner();
            let coin_type = "0x2::sui:🤵";
            let mock_internal = MockCoinReadInternal::new();
            let coin_read_api = CoinReadApi {
                internal: Box::new(mock_internal),
            };

            let response = coin_read_api
                .get_coins(owner, Some(coin_type.to_string()), None, None)
                .await;

            assert!(response.is_err());
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();
            assert_eq!(error_object.code(), -32602);
            assert_eq!(
                error_object.message(),
                "Invalid struct type: 0x2::sui:🤵. Got error: unrecognized token: :🤵"
            );
        }

        #[tokio::test]
        async fn test_get_coins_iterator_index_store_not_available() {
            let owner = get_test_owner();
            let coin_type = get_test_coin_type(get_test_package_id());
            let mut mock_state = MockState::new();
            mock_state
                .expect_get_owned_coins()
                .returning(move |_, _, _, _| Err(SuiError::IndexStoreNotAvailable));
            let internal = CoinReadInternalImpl {
                state: Arc::new(mock_state),
                metrics: Arc::new(JsonRpcMetrics::new_for_tests()),
            };
            let coin_read_api = CoinReadApi {
                internal: Box::new(internal),
            };

            let response = coin_read_api
                .get_coins(owner, Some(coin_type.to_string()), None, None)
                .await;

            assert!(response.is_err());
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();

            assert_eq!(error_object.code(), -32000);
            assert_eq!(
                error_object.message(),
                "Index store not available on this Fullnode."
            );
        }

        #[tokio::test]
        async fn test_get_coins_iterator_typed_store_error() {
            let owner = get_test_owner();
            let coin_type = get_test_coin_type(get_test_package_id());
            let mut mock_state = MockState::new();
            mock_state
                .expect_get_owned_coins()
                .returning(move |_, _, _, _| {
                    Err(TypedStoreError::RocksDBError("mock rocksdb error".to_string()).into())
                });
            let internal = CoinReadInternalImpl {
                state: Arc::new(mock_state),
                metrics: Arc::new(JsonRpcMetrics::new_for_tests()),
            };
            let coin_read_api = CoinReadApi {
                internal: Box::new(internal),
            };

            let response = coin_read_api
                .get_coins(owner, Some(coin_type.to_string()), None, None)
                .await;

            assert!(response.is_err());
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();

            assert_eq!(error_object.code(), -32000);
            assert_eq!(error_object.message(), "Storage error");
        }
    }

    mod get_all_coins_tests {
        use super::super::*;
        use super::*;

        #[tokio::test]
        async fn test_object_is_not_coin() {
            let owner = get_test_owner();
            let object_id = get_test_package_id();
            let (_, _, _, _, treasury_cap_object) = get_test_treasury_cap_peripherals(object_id);
            let mut mock_state = MockState::new();
            mock_state.expect_get_object().returning(move |obj_id| {
                if obj_id == &object_id {
                    Ok(Some(treasury_cap_object.clone()))
                } else {
                    panic!("should not be called with any other object id")
                }
            });
            let internal = CoinReadInternalImpl {
                state: Arc::new(mock_state),
                metrics: Arc::new(JsonRpcMetrics::new_for_tests()),
            };
            let coin_read_api = CoinReadApi {
                internal: Box::new(internal),
            };

            let response = coin_read_api
                .get_all_coins(owner, Some(object_id), None)
                .await;

            assert!(response.is_err());
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();
            assert_eq!(error_object.code(), -32602);
            assert_eq!(
                error_object.message(),
                format!("Invalid Cursor {:?}, Object is not a coin", object_id)
            );
        }

        #[tokio::test]
        async fn test_object_not_found() {
            let owner = get_test_owner();
            let object_id = ObjectID::random();
            let mut mock_state = MockState::new();
            mock_state.expect_get_object().returning(move |_| Ok(None));

            let internal = CoinReadInternalImpl {
                state: Arc::new(mock_state),
                metrics: Arc::new(JsonRpcMetrics::new_for_tests()),
            };

            let coin_read_api = CoinReadApi {
                internal: Box::new(internal),
            };

            let response = coin_read_api
                .get_all_coins(owner, Some(object_id), None)
                .await;

            assert!(response.is_err());
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();
            assert_eq!(error_object.code(), -32602);
            assert_eq!(
                error_object.message(),
                format!("Invalid Cursor {:?}, Object not found", object_id)
            );
        }
    }

    mod get_balance_tests {
        use super::super::*;
        use super::*;
        use jsonrpsee::types::ErrorObjectOwned;

        #[tokio::test]
        async fn test_get_balance_index_store_not_available() {
            let owner = get_test_owner();
            let coin_type = get_test_coin_type(get_test_package_id());
            let mut mock_state = MockState::new();
            mock_state
                .expect_get_balance()
                .returning(move |_, _| Err(SuiError::IndexStoreNotAvailable));
            let internal = CoinReadInternalImpl {
                state: Arc::new(mock_state),
                metrics: Arc::new(JsonRpcMetrics::new_for_tests()),
            };
            let coin_read_api = CoinReadApi {
                internal: Box::new(internal),
            };

            let response = coin_read_api
                .get_balance(owner, Some(coin_type.to_string()))
                .await;

            assert!(response.is_err());
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();
            assert_eq!(error_object.code(), -32000);
            assert_eq!(
                error_object.message(),
                "Index store not available on this Fullnode."
            );
        }

        #[tokio::test]
        async fn test_get_balance_execution_error() {
            let owner = get_test_owner();
            let coin_type = get_test_coin_type(get_test_package_id());
            let mut mock_state = MockState::new();
            mock_state
                .expect_get_balance()
                .returning(move |_, _| Err(SuiError::ExecutionError("mock db error".to_string())));
            let internal = CoinReadInternalImpl {
                state: Arc::new(mock_state),
                metrics: Arc::new(JsonRpcMetrics::new_for_tests()),
            };
            let coin_read_api = CoinReadApi {
                internal: Box::new(internal),
            };

            let response = coin_read_api
                .get_balance(owner, Some(coin_type.to_string()))
                .await;

            assert!(response.is_err());
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();

            assert_eq!(error_object.code(), -32000);
            assert_eq!(error_object.message(), "Error executing mock db error");
        }
    }

    mod get_coin_metadata_tests {
        use super::super::*;
        use super::*;
        use mockall::predicate;
        use sui_types::id::UID;

        #[tokio::test]
        async fn test_find_package_object_not_sui_coin_metadata() {
            let package_id = get_test_package_id();
            let coin_name = get_test_coin_type(package_id);
            let input_coin_struct = parse_sui_struct_tag(&coin_name).expect("should not fail");
            let coin_metadata_struct = CoinMetadata::type_(input_coin_struct.clone());
            let treasury_cap = TreasuryCap {
                id: UID::new(ObjectID::random()),
                total_supply: Supply { value: 420 },
            };
            let treasury_cap_object =
                Object::treasury_cap_for_testing(input_coin_struct.clone(), treasury_cap);
            let mut mock_state = MockState::new();
            // return TreasuryCap instead of CoinMetadata to set up test
            mock_state
                .expect_find_package_object()
                .with(predicate::always(), predicate::eq(coin_metadata_struct))
                .returning(move |object_id, _| {
                    if object_id == &package_id {
                        Ok(treasury_cap_object.clone())
                    } else {
                        panic!("should not be called with any other object id")
                    }
                });
            let internal = CoinReadInternalImpl {
                state: Arc::new(mock_state),
                metrics: Arc::new(JsonRpcMetrics::new_for_tests()),
            };
            let coin_read_api = CoinReadApi {
                internal: Box::new(internal),
            };

            let response = coin_read_api.get_coin_metadata(coin_name.clone()).await;
            assert!(response.is_ok());
            let result = response.unwrap();
            assert!(result.is_none());
        }
    }

    mod get_total_supply_tests {
        use super::super::*;
        use super::*;
        use mockall::predicate;
        use sui_types::id::UID;

        #[tokio::test]
        async fn test_success_response_for_gas_coin() {
            let coin_type = "0x2::sui::SUI";
            let mock_internal = MockCoinReadInternal::new();
            let coin_read_api = CoinReadApi {
                internal: Box::new(mock_internal),
            };

            let response = coin_read_api.get_total_supply(coin_type.to_string()).await;

            let supply = response.unwrap();
            assert_eq!(supply.value, 0);
        }

        #[tokio::test]
        async fn test_success_response_for_other_coin() {
            let package_id = get_test_package_id();
            let (coin_name, _, treasury_cap_struct, _, treasury_cap_object) =
                get_test_treasury_cap_peripherals(package_id);
            let mut mock_state = MockState::new();
            mock_state
                .expect_find_package_object()
                .with(predicate::always(), predicate::eq(treasury_cap_struct))
                .returning(move |object_id, _| {
                    if object_id == &package_id {
                        Ok(treasury_cap_object.clone())
                    } else {
                        panic!("should not be called with any other object id")
                    }
                });
            let internal = CoinReadInternalImpl {
                state: Arc::new(mock_state),
                metrics: Arc::new(JsonRpcMetrics::new_for_tests()),
            };
            let coin_read_api = CoinReadApi {
                internal: Box::new(internal),
            };

            let response = coin_read_api.get_total_supply(coin_name.clone()).await;

            assert!(response.is_ok());
            let result = response.unwrap();
            assert_eq!(result.value, 420);
        }

        #[tokio::test]
        async fn test_find_package_object_not_treasury_cap() {
            let package_id = get_test_package_id();
            let (coin_name, input_coin_struct, treasury_cap_struct, _, _) =
                get_test_treasury_cap_peripherals(package_id);
            let coin_metadata = CoinMetadata {
                id: UID::new(ObjectID::random()),
                decimals: 2,
                name: "test_coin".to_string(),
                symbol: "TEST".to_string(),
                description: "test coin".to_string(),
                icon_url: None,
            };
            let coin_metadata_object =
                Object::coin_metadata_for_testing(input_coin_struct.clone(), coin_metadata);
            let mut mock_state = MockState::new();
            mock_state
                .expect_find_package_object()
                .with(predicate::always(), predicate::eq(treasury_cap_struct))
                .returning(move |object_id, _| {
                    if object_id == &package_id {
                        Ok(coin_metadata_object.clone())
                    } else {
                        panic!("should not be called with any other object id")
                    }
                });
            let internal = CoinReadInternalImpl {
                state: Arc::new(mock_state),
                metrics: Arc::new(JsonRpcMetrics::new_for_tests()),
            };
            let coin_read_api = CoinReadApi {
                internal: Box::new(internal),
            };

            let response = coin_read_api.get_total_supply(coin_name.clone()).await;
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();
            assert!(error_object.code() == -32000);
            assert!(error_object.message() == "Failure deserializing object in the requested format: \"Unable to deserialize TreasuryCap object: remaining input\"");
        }
    }
}
