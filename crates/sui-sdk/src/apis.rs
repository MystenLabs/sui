// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::future;
use std::sync::Arc;
use std::time::Instant;

use fastcrypto::encoding::Base64;
use futures::stream;
use futures::StreamExt;
use futures_core::Stream;
use jsonrpsee::core::client::Subscription;

use crate::error::{Error, SuiRpcResult};
use crate::RpcClient;
use sui_json_rpc::api::GovernanceReadApiClient;
use sui_json_rpc::api::{
    CoinReadApiClient, IndexerApiClient, MoveUtilsClient, ReadApiClient, WriteApiClient,
};
use sui_json_rpc_types::{
    Balance, Checkpoint, CheckpointId, Coin, CoinPage, DelegatedStake, DevInspectResults,
    DryRunTransactionBlockResponse, DynamicFieldPage, EventFilter, EventPage, ObjectsPage,
    ProtocolConfigResponse, SuiCoinMetadata, SuiCommittee, SuiEvent, SuiGetPastObjectRequest,
    SuiMoveNormalizedModule, SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery,
    SuiPastObjectResponse, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
    SuiTransactionBlockResponseQuery, TransactionBlocksPage,
};
use sui_json_rpc_types::{CheckpointPage, SuiLoadedChildObjectsResponse};
use sui_types::balance::Supply;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::event::EventID;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::sui_serde::BigInt;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_types::transaction::{Transaction, TransactionData, TransactionKind};

const WAIT_FOR_LOCAL_EXECUTION_RETRY_COUNT: u8 = 3;

/// The main read API structure with functions for retriving data about different objects and transactions
#[derive(Debug)]
pub struct ReadApi {
    api: Arc<RpcClient>,
}

impl ReadApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }
    /// Return a paginated response containing the owned objects for this Sui address, or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::SuiAddress;
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let owned_objects = sui.read_api().get_owned_objects(address, None, None, None).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> SuiRpcResult<ObjectsPage> {
        Ok(self
            .api
            .http
            .get_owned_objects(address, query, cursor, limit)
            .await?)
    }

    /// Return a paginated response containing the dynamic fields objects for this ObjectID, or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::{ObjectID, SuiAddress};
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // connect to the local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?;
    ///     let owned_objects = sui.read_api().get_owned_objects(address, None, None, None).await?;
    ///     // this code example assumes that there are previous owned objects, otherwise it panics
    ///     let object = owned_objects.data.get(0).expect(&format!(
    ///         "No owned objects for this address {}",
    ///         address
    ///     ));
    ///     let object_data = object.data.as_ref().expect(&format!(
    ///         "No object data for this SuiObjectResponse {:?}",
    ///         object
    ///     ));
    ///     let object_id = object_data.object_id;
    ///     let dynamic_fields = sui
    ///         .read_api()
    ///         .get_dynamic_fields(object_id, None, None)
    ///         .await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_dynamic_fields(
        &self,
        object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> SuiRpcResult<DynamicFieldPage> {
        Ok(self
            .api
            .http
            .get_dynamic_fields(object_id, cursor, limit)
            .await?)
    }

    /// Return a parsed past object for the provided [ObjectID], or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::{ObjectID, SuiAddress};
    /// use sui_json_rpc_types::SuiObjectDataOptions;
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    ///     async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // connect to the local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let owned_objects = sui.read_api().get_owned_objects(address, None, None, None).await?;
    ///     // this code example assumes that there are previous owned objects, otherwise it panics
    ///     let object = owned_objects.data.get(0).expect(&format!(
    ///         "No owned objects for this address {}",
    ///         address
    ///     ));
    ///     let object_data = object.data.as_ref().expect(&format!(
    ///         "No object data for this SuiObjectResponse {:?}",
    ///         object
    ///     ));
    ///     let object_id = object_data.object_id;
    ///     let version = object_data.version;
    ///     let past_object = sui
    ///         .read_api()
    ///         .try_get_parsed_past_object(
    ///             object_id,
    ///             version,
    ///             SuiObjectDataOptions {
    ///                 show_type: true,
    ///                 show_owner: true,
    ///                 show_previous_transaction: true,
    ///                 show_display: true,
    ///                 show_content: true,
    ///                 show_bcs: true,
    ///                 show_storage_rebate: true,
    ///             },
    ///         )
    ///         .await?;
    ///     Ok(())
    /// }
    ///```
    pub async fn try_get_parsed_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: SuiObjectDataOptions,
    ) -> SuiRpcResult<SuiPastObjectResponse> {
        Ok(self
            .api
            .http
            .try_get_past_object(object_id, version, Some(options))
            .await?)
    }

    /// Return a vector containing [SuiPastObjectResponse] objects, or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::{ObjectID, SuiAddress};
    /// use sui_json_rpc_types::{SuiObjectDataOptions, SuiGetPastObjectRequest};
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // connect to the local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let owned_objects = sui.read_api().get_owned_objects(address, None, None, None).await?;
    ///     // this code example assumes that there are previous owned objects, otherwise it panics
    ///     let object = owned_objects.data.get(0).expect(&format!(
    ///         "No owned objects for this address {}",
    ///         address
    ///     ));
    ///     let object_data = object.data.as_ref().expect(&format!(
    ///         "No object data for this SuiObjectResponse {:?}",
    ///         object
    ///     ));
    ///     let object_id = object_data.object_id;
    ///     let version = object_data.version;
    ///     let past_object = sui
    ///         .read_api()
    ///         .try_get_parsed_past_object(
    ///             object_id,
    ///             version,
    ///             SuiObjectDataOptions {
    ///                 show_type: true,
    ///                 show_owner: true,
    ///                 show_previous_transaction: true,
    ///                 show_display: true,
    ///                 show_content: true,
    ///                 show_bcs: true,
    ///                 show_storage_rebate: true,
    ///             },
    ///         )
    ///         .await?;
    ///     let past_object = past_object.into_object()?;
    ///     let multi_past_object = sui
    ///         .read_api()
    ///         .try_multi_get_parsed_past_object(
    ///             vec![SuiGetPastObjectRequest {
    ///                 object_id: past_object.object_id,
    ///                 version: past_object.version,
    ///             }],
    ///             SuiObjectDataOptions {
    ///                 show_type: true,
    ///                 show_owner: true,
    ///                 show_previous_transaction: true,
    ///                 show_display: true,
    ///                 show_content: true,
    ///                 show_bcs: true,
    ///                 show_storage_rebate: true,
    ///             },
    ///         )
    ///         .await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn try_multi_get_parsed_past_object(
        &self,
        past_objects: Vec<SuiGetPastObjectRequest>,
        options: SuiObjectDataOptions,
    ) -> SuiRpcResult<Vec<SuiPastObjectResponse>> {
        Ok(self
            .api
            .http
            .try_multi_get_past_objects(past_objects, Some(options))
            .await?)
    }

    /// Return a [SuiObjectResponse] based on the provided [ObjectID] and [SuiObjectDataOptions], or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::SuiAddress;
    /// use sui_json_rpc_types::SuiObjectDataOptions;
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let owned_objects = sui.read_api().get_owned_objects(address, None, None, None).await?;
    ///     // this code example assumes that there are previous owned objects, otherwise it panics
    ///     let object = owned_objects.data.get(0).expect(&format!(
    ///         "No owned objects for this address {}",
    ///         address
    ///     ));
    ///     let object_data = object.data.as_ref().expect(&format!(
    ///         "No object data for this SuiObjectResponse {:?}",
    ///         object
    ///     ));
    ///     let object_id = object_data.object_id;
    ///     let object = sui.read_api().get_object_with_options(object_id,
    ///             SuiObjectDataOptions {
    ///                 show_type: true,
    ///                 show_owner: true,
    ///                 show_previous_transaction: true,
    ///                 show_display: true,
    ///                 show_content: true,
    ///                 show_bcs: true,
    ///                 show_storage_rebate: true,
    ///             },
    ///         ).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: SuiObjectDataOptions,
    ) -> SuiRpcResult<SuiObjectResponse> {
        Ok(self.api.http.get_object(object_id, Some(options)).await?)
    }

    /// Return a vector of [SuiObjectResponse] based on the given vector of [ObjectID]s and [SuiObjectDataOptions], or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::SuiAddress;
    /// use sui_json_rpc_types::SuiObjectDataOptions;
    /// use std::str::FromStr;
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let owned_objects = sui.read_api().get_owned_objects(address, None, None, None).await?;
    ///     // this code example assumes that there are previous owned objects, otherwise it panics
    ///     let object = owned_objects.data.get(0).expect(&format!(
    ///         "No owned objects for this address {}",
    ///         address
    ///     ));
    ///     let object_data = object.data.as_ref().expect(&format!(
    ///         "No object data for this SuiObjectResponse {:?}",
    ///         object
    ///     ));
    ///     let object_id = object_data.object_id;
    ///     let object_ids = vec![object_id]; // and other object ids
    ///     let object = sui.read_api().multi_get_object_with_options(object_ids,
    ///             SuiObjectDataOptions {
    ///                 show_type: true,
    ///                 show_owner: true,
    ///                 show_previous_transaction: true,
    ///                 show_display: true,
    ///                 show_content: true,
    ///                 show_bcs: true,
    ///                 show_storage_rebate: true,
    ///             },
    ///         ).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn multi_get_object_with_options(
        &self,
        object_ids: Vec<ObjectID>,
        options: SuiObjectDataOptions,
    ) -> SuiRpcResult<Vec<SuiObjectResponse>> {
        Ok(self
            .api
            .http
            .multi_get_objects(object_ids, Some(options))
            .await?)
    }

    /// Return the total number of transaction blocks, or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let total_transaction_blocks = sui.read_api().get_total_transaction_blocks().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_total_transaction_blocks(&self) -> SuiRpcResult<u64> {
        Ok(*self.api.http.get_total_transaction_blocks().await?)
    }

    /// Return a [SuiTransactionBlockResponse] based on the given TransactionDigest, or an error upon failure.
    pub async fn get_transaction_with_options(
        &self,
        digest: TransactionDigest,
        options: SuiTransactionBlockResponseOptions,
    ) -> SuiRpcResult<SuiTransactionBlockResponse> {
        Ok(self
            .api
            .http
            .get_transaction_block(digest, Some(options))
            .await?)
    }
    /// Return a vector of SuiTransactionBlockResponse based on the given list of TransactionDigest, or an error upon failure.
    pub async fn multi_get_transactions_with_options(
        &self,
        digests: Vec<TransactionDigest>,
        options: SuiTransactionBlockResponseOptions,
    ) -> SuiRpcResult<Vec<SuiTransactionBlockResponse>> {
        Ok(self
            .api
            .http
            .multi_get_transaction_blocks(digests, Some(options))
            .await?)
    }

    /// Return the [SuiCommittee] information for the provided `epoch`, or an error upon failure.
    ///
    /// # Arguments
    ///
    /// * `epoch` - the known epoch id or `None` for the last epoch
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let committee_info = sui.read_api().get_committee_info(None).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_committee_info(
        &self,
        epoch: Option<BigInt<u64>>,
    ) -> SuiRpcResult<SuiCommittee> {
        Ok(self.api.http.get_committee_info(epoch).await?)
    }

    // Return a paginated response containing all transaction blocks information, or an error upon failure.
    pub async fn query_transaction_blocks(
        &self,
        query: SuiTransactionBlockResponseQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> SuiRpcResult<TransactionBlocksPage> {
        Ok(self
            .api
            .http
            .query_transaction_blocks(query, cursor, limit, Some(descending_order))
            .await?)
    }

    // Return the chain identifier, or an error upon failure.
    pub async fn get_chain_identifier(&self) -> SuiRpcResult<String> {
        Ok(self.api.http.get_chain_identifier().await?)
    }

    /// Return a checkpoint, or an error upon failure.
    pub async fn get_checkpoint(&self, id: CheckpointId) -> SuiRpcResult<Checkpoint> {
        Ok(self.api.http.get_checkpoint(id).await?)
    }

    /// Return paginated list of checkpoints, or an error upon failure.
    pub async fn get_checkpoints(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> SuiRpcResult<CheckpointPage> {
        Ok(self
            .api
            .http
            .get_checkpoints(cursor, limit, descending_order)
            .await?)
    }

    /// Return the sequence number of the latest checkpoint that has been executed, or an error upon failure.
    pub async fn get_latest_checkpoint_sequence_number(
        &self,
    ) -> SuiRpcResult<CheckpointSequenceNumber> {
        Ok(*self
            .api
            .http
            .get_latest_checkpoint_sequence_number()
            .await?)
    }

    /// Return a stream of transaction block response, or an error upon failure.
    pub fn get_transactions_stream(
        &self,
        query: SuiTransactionBlockResponseQuery,
        cursor: Option<TransactionDigest>,
        descending_order: bool,
    ) -> impl Stream<Item = SuiTransactionBlockResponse> + '_ {
        stream::unfold(
            (vec![], cursor, true, query),
            move |(mut data, cursor, first, query)| async move {
                if let Some(item) = data.pop() {
                    Some((item, (data, cursor, false, query)))
                } else if (cursor.is_none() && first) || cursor.is_some() {
                    let page = self
                        .query_transaction_blocks(
                            query.clone(),
                            cursor,
                            Some(100),
                            descending_order,
                        )
                        .await
                        .ok()?;
                    let mut data = page.data;
                    data.reverse();
                    data.pop()
                        .map(|item| (item, (data, page.next_cursor, false, query)))
                } else {
                    None
                }
            },
        )
    }

    /// Return a map consisting of the move package name and the normalized module, or an error upon failure.
    pub async fn get_normalized_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> SuiRpcResult<BTreeMap<String, SuiMoveNormalizedModule>> {
        Ok(self
            .api
            .http
            .get_normalized_move_modules_by_package(package)
            .await?)
    }

    // TODO(devx): we can probably cache this given an epoch
    /// Return the reference gas price, or an error upon failure.
    pub async fn get_reference_gas_price(&self) -> SuiRpcResult<u64> {
        Ok(*self.api.http.get_reference_gas_price().await?)
    }

    /// Dry run a transaction block given the provided transaction data. Returns an error upon failure.
    pub async fn dry_run_transaction_block(
        &self,
        tx: TransactionData,
    ) -> SuiRpcResult<DryRunTransactionBlockResponse> {
        Ok(self
            .api
            .http
            .dry_run_transaction_block(Base64::from_bytes(&bcs::to_bytes(&tx)?))
            .await?)
    }

    pub async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        tx: TransactionKind,
        gas_price: Option<BigInt<u64>>,
        epoch: Option<BigInt<u64>>,
    ) -> SuiRpcResult<DevInspectResults> {
        Ok(self
            .api
            .http
            .dev_inspect_transaction_block(
                sender_address,
                Base64::from_bytes(&bcs::to_bytes(&tx)?),
                gas_price,
                epoch,
            )
            .await?)
    }

    /// Return the loaded child objects response based on the provided digest, or an error upon failure.
    pub async fn get_loaded_child_objects(
        &self,
        digest: TransactionDigest,
    ) -> SuiRpcResult<SuiLoadedChildObjectsResponse> {
        Ok(self.api.http.get_loaded_child_objects(digest).await?)
    }

    /// Return the protocol config, or an error upon failure.
    pub async fn get_protocol_config(
        &self,
        version: Option<BigInt<u64>>,
    ) -> SuiRpcResult<ProtocolConfigResponse> {
        Ok(self.api.http.get_protocol_config(version).await?)
    }
}

/// Coin Read API provides the functionality needed to get information from the Sui network regarding the coins owned by an address.
#[derive(Debug, Clone)]
pub struct CoinReadApi {
    api: Arc<RpcClient>,
}

impl CoinReadApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }

    /// Return a list of coins for the provided address in a paginated fashion, or an error upon failure.
    ///
    /// The coins can be filtered by `coin_type` or use `None` for including all coin types.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::SuiAddress;
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let coins = sui.coin_read_api().get_coins(address, None, None, None).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> SuiRpcResult<CoinPage> {
        Ok(self
            .api
            .http
            .get_coins(owner, coin_type, cursor, limit)
            .await?)
    }
    /// Return all coins in a paginated fashion, or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::SuiAddress;
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let coins = sui.coin_read_api().get_all_coins(address, None, None).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_all_coins(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> SuiRpcResult<CoinPage> {
        Ok(self.api.http.get_all_coins(owner, cursor, limit).await?)
    }

    /// Return the coins for the provided address as a stream.
    ///
    /// The coins can be filtered by `coin_type` or use `None` for including all coin types.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::SuiAddress;
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let coins = sui.coin_read_api().get_coins_stream(address, None);
    ///     Ok(())
    /// }
    /// ```
    pub fn get_coins_stream(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> impl Stream<Item = Coin> + '_ {
        stream::unfold(
            (
                vec![],
                /* cursor */ None,
                /* has_next_page */ true,
                coin_type,
            ),
            move |(mut data, cursor, has_next_page, coin_type)| async move {
                if let Some(item) = data.pop() {
                    Some((item, (data, cursor, /* has_next_page */ true, coin_type)))
                } else if has_next_page {
                    let page = self
                        .get_coins(owner, coin_type.clone(), cursor, Some(100))
                        .await
                        .ok()?;
                    let mut data = page.data;
                    data.reverse();
                    data.pop().map(|item| {
                        (
                            item,
                            (data, page.next_cursor, page.has_next_page, coin_type),
                        )
                    })
                } else {
                    None
                }
            },
        )
    }

    /// Return a list of coins for the provided address, or an error upon failure.
    ///
    /// The coins can be filtered by `coin_type` or use `None` for including all coin types.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::SuiAddress;
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let coins = sui.coin_read_api().select_coins(address, None, 5, vec![]).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn select_coins(
        &self,
        address: SuiAddress,
        coin_type: Option<String>,
        amount: u128,
        exclude: Vec<ObjectID>,
    ) -> SuiRpcResult<Vec<Coin>> {
        let mut total = 0u128;
        let coins = self
            .get_coins_stream(address, coin_type)
            .filter(|coin: &Coin| future::ready(!exclude.contains(&coin.coin_object_id)))
            .take_while(|coin: &Coin| {
                let ready = future::ready(total < amount);
                total += coin.balance as u128;
                ready
            })
            .collect::<Vec<_>>()
            .await;

        if total < amount {
            return Err(Error::InsufficientFund { address, amount });
        }
        Ok(coins)
    }

    /// Return the balance of coins for the provided address, or an error upon failure.
    ///
    /// The coins can be filtered by coin type or use `None` for including all coin types.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::SuiAddress;
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let balance = sui.coin_read_api().get_balance(address, None).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_balance(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> SuiRpcResult<Balance> {
        Ok(self.api.http.get_balance(owner, coin_type).await?)
    }

    /// Return the total balance of all coins for the provided address, or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// use sui_types::base_types::SuiAddress;
    /// use std::str::FromStr;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let address = SuiAddress::from_str("0x0000....0000")?; // change to your Sui address
    ///     let all_balances = sui.coin_read_api().get_all_balances(address).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_all_balances(&self, owner: SuiAddress) -> SuiRpcResult<Vec<Balance>> {
        Ok(self.api.http.get_all_balances(owner).await?)
    }

    /// Return the coin metadata for a given coin type, or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let coin_metadata = sui.coin_read_api().get_coin_metadata("0x2::sui::SUI".to_string()).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_coin_metadata(
        &self,
        coin_type: String,
    ) -> SuiRpcResult<Option<SuiCoinMetadata>> {
        Ok(self.api.http.get_coin_metadata(coin_type).await?)
    }

    /// Return the total supply for a given coin type, or an error upon failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let total_supply = sui.coin_read_api().get_total_supply("0x2::sui::SUI".to_string()).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_total_supply(&self, coin_type: String) -> SuiRpcResult<Supply> {
        Ok(self.api.http.get_total_supply(coin_type).await?)
    }
}

/// Event API provides the functionality to fetch, query, or subscribe to events on the Sui network.
#[derive(Clone)]
pub struct EventApi {
    api: Arc<RpcClient>,
}

impl EventApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }

    /// Return a stream of events, or an error upon failure.
    pub async fn subscribe_event(
        &self,
        filter: EventFilter,
    ) -> SuiRpcResult<impl Stream<Item = SuiRpcResult<SuiEvent>>> {
        match &self.api.ws {
            Some(c) => {
                let subscription: Subscription<SuiEvent> = c.subscribe_event(filter).await?;
                Ok(subscription.map(|item| Ok(item?)))
            }
            _ => Err(Error::Subscription(
                "Subscription only supported by WebSocket client.".to_string(),
            )),
        }
    }

    /// Return a list of events based on the transaction digest, or an error upon failure.
    pub async fn get_events(&self, digest: TransactionDigest) -> SuiRpcResult<Vec<SuiEvent>> {
        Ok(self.api.http.get_events(digest).await?)
    }

    /// Return a paginated list of events based on the given event filter, or an error upon failure.
    pub async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> SuiRpcResult<EventPage> {
        Ok(self
            .api
            .http
            .query_events(query, cursor, limit, Some(descending_order))
            .await?)
    }

    /// Return a stream of events based on the given event filter.
    ///
    /// By default, each page will return 100 items.
    pub fn get_events_stream(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
        descending_order: bool,
    ) -> impl Stream<Item = SuiEvent> + '_ {
        stream::unfold(
            (vec![], cursor, true, query),
            move |(mut data, cursor, first, query)| async move {
                if let Some(item) = data.pop() {
                    Some((item, (data, cursor, false, query)))
                } else if (cursor.is_none() && first) || cursor.is_some() {
                    let page = self
                        .query_events(query.clone(), cursor, Some(100), descending_order)
                        .await
                        .ok()?;
                    let mut data = page.data;
                    data.reverse();
                    data.pop()
                        .map(|item| (item, (data, page.next_cursor, false, query)))
                } else {
                    None
                }
            },
        )
    }
}

/// Quorum API that provides functionality to execute a transaction block and submit it to the fullnode(s).
#[derive(Clone)]
pub struct QuorumDriverApi {
    api: Arc<RpcClient>,
}

impl QuorumDriverApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }

    /// Execute a transaction with a FullNode client. `request_type`
    /// defaults to `ExecuteTransactionRequestType::WaitForLocalExecution`.
    /// When `ExecuteTransactionRequestType::WaitForLocalExecution` is used,
    /// but returned `confirmed_local_execution` is false, the client will
    /// keep retry for WAIT_FOR_LOCAL_EXECUTION_RETRY_COUNT times. If it
    /// still fails, it will return an error.
    pub async fn execute_transaction_block(
        &self,
        tx: Transaction,
        options: SuiTransactionBlockResponseOptions,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> SuiRpcResult<SuiTransactionBlockResponse> {
        let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();
        let request_type = request_type.unwrap_or_else(|| options.default_execution_request_type());
        let mut retry_count = 0;
        let start = Instant::now();
        while retry_count < WAIT_FOR_LOCAL_EXECUTION_RETRY_COUNT {
            let response: SuiTransactionBlockResponse = self
                .api
                .http
                .execute_transaction_block(
                    tx_bytes.clone(),
                    signatures.clone(),
                    Some(options.clone()),
                    Some(request_type.clone()),
                )
                .await?;

            match request_type {
                ExecuteTransactionRequestType::WaitForEffectsCert => {
                    return Ok(response);
                }
                ExecuteTransactionRequestType::WaitForLocalExecution => {
                    if let Some(true) = response.confirmed_local_execution {
                        return Ok(response);
                    } else {
                        // If fullnode executed the cert in the network but did not confirm local
                        // execution, it must have timed out and hence we could retry.
                        retry_count += 1;
                    }
                }
            }
        }
        Err(Error::FailToConfirmTransactionStatus(
            *tx.digest(),
            start.elapsed().as_secs(),
        ))
    }
}

/// Governance API provides the functionality needed for staking on the Sui network.
#[derive(Debug, Clone)]
pub struct GovernanceApi {
    api: Arc<RpcClient>,
}

impl GovernanceApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }

    /// Return all [DelegatedStake].
    pub async fn get_stakes(&self, owner: SuiAddress) -> SuiRpcResult<Vec<DelegatedStake>> {
        Ok(self.api.http.get_stakes(owner).await?)
    }

    /// Return the [SuiCommittee] information for the provided `epoch`, or an error upon failure.
    ///
    /// # Arguments
    ///
    /// * `epoch` - the known epoch id or `None` for the last epoch
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sui_sdk::SuiClientBuilder;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), anyhow::Error> {     
    ///     let sui = SuiClientBuilder::default().build_localnet().await?; // local Sui network
    ///     let committee_info = sui.read_api().get_committee_info(None).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_committee_info(
        &self,
        epoch: Option<BigInt<u64>>,
    ) -> SuiRpcResult<SuiCommittee> {
        Ok(self.api.http.get_committee_info(epoch).await?)
    }

    /// Return the latest SUI system state object on-chain, or an error upon failure.
    pub async fn get_latest_sui_system_state(&self) -> SuiRpcResult<SuiSystemStateSummary> {
        Ok(self.api.http.get_latest_sui_system_state().await?)
    }

    /// Return the reference gas price for the network, or an error upon failure.
    pub async fn get_reference_gas_price(&self) -> SuiRpcResult<u64> {
        Ok(*self.api.http.get_reference_gas_price().await?)
    }
}
