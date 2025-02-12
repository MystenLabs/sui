// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use sui_json_rpc::error::SuiRpcInputError;
use sui_types::error::SuiObjectResponseError;
use sui_types::object::ObjectRead;

use crate::errors::IndexerError;
use crate::indexer_reader::IndexerReader;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_api::{ReadApiServer, QUERY_MAX_RESULT_LIMIT};
use sui_json_rpc_types::ZkLoginIntentScope;
use sui_json_rpc_types::ZkLoginVerifyResult;
use sui_json_rpc_types::{
    Checkpoint, CheckpointId, CheckpointPage, ProtocolConfigResponse, SuiEvent,
    SuiGetPastObjectRequest, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_types::base_types::SuiAddress;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::{ChainIdentifier, TransactionDigest};
use sui_types::sui_serde::BigInt;

#[derive(Clone)]
pub struct ReadApi {
    inner: IndexerReader,
}

impl ReadApi {
    pub fn new(inner: IndexerReader) -> Self {
        Self { inner }
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> Result<Checkpoint, IndexerError> {
        match self.inner.get_checkpoint(id).await {
            Ok(Some(epoch_info)) => Ok(epoch_info),
            Ok(None) => Err(IndexerError::InvalidArgumentError(format!(
                "Checkpoint {id:?} not found"
            ))),
            Err(e) => Err(e),
        }
    }

    async fn get_latest_checkpoint(&self) -> Result<Checkpoint, IndexerError> {
        self.inner.get_latest_checkpoint().await
    }

    async fn get_chain_identifier(&self) -> RpcResult<ChainIdentifier> {
        let genesis_checkpoint = self.get_checkpoint(CheckpointId::SequenceNumber(0)).await?;
        Ok(ChainIdentifier::from(genesis_checkpoint.digest))
    }
}

#[async_trait]
impl ReadApiServer for ReadApi {
    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        let object_read = self.inner.get_object_read(object_id).await?;
        object_read_to_object_response(&self.inner, object_read, options.unwrap_or_default()).await
    }

    // For ease of implementation we just forward to the single object query, although in the
    // future we may want to improve the performance by having a more naitive multi_get
    // functionality
    async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        if object_ids.len() > *QUERY_MAX_RESULT_LIMIT {
            return Err(
                SuiRpcInputError::SizeLimitExceeded(QUERY_MAX_RESULT_LIMIT.to_string()).into(),
            );
        }
        let stored_objects = self.inner.multi_get_objects(object_ids).await?;
        let options = options.unwrap_or_default();

        let futures = stored_objects.into_iter().map(|stored_object| async {
            let object_read = stored_object
                .try_into_object_read(self.inner.package_resolver())
                .await?;
            object_read_to_object_response(&self.inner, object_read, options.clone()).await
        });

        let mut objects = futures::future::try_join_all(futures).await?;
        // Resort the objects by the order of the object id.
        objects.sort_by_key(|obj| obj.data.as_ref().map(|data| data.object_id));

        Ok(objects)
    }

    async fn get_total_transaction_blocks(&self) -> RpcResult<BigInt<u64>> {
        let checkpoint = self.get_latest_checkpoint().await?;
        Ok(BigInt::from(checkpoint.network_total_transactions))
    }

    async fn get_transaction_block(
        &self,
        digest: TransactionDigest,
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        let mut txn = self
            .multi_get_transaction_blocks(vec![digest], options)
            .await?;

        let txn = txn.pop().ok_or_else(|| {
            IndexerError::InvalidArgumentError(format!("Transaction {digest} not found"))
        })?;

        Ok(txn)
    }

    async fn multi_get_transaction_blocks(
        &self,
        digests: Vec<TransactionDigest>,
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionBlockResponse>> {
        let num_digests = digests.len();
        if num_digests > *QUERY_MAX_RESULT_LIMIT {
            Err(SuiRpcInputError::SizeLimitExceeded(
                QUERY_MAX_RESULT_LIMIT.to_string(),
            ))?
        }

        let options = options.unwrap_or_default();
        let txns = self
            .inner
            .multi_get_transaction_block_response_in_blocking_task(digests, options)
            .await?;

        Ok(txns)
    }

    async fn try_get_past_object(
        &self,
        _object_id: ObjectID,
        _version: SequenceNumber,
        _options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        Err(jsonrpsee::types::error::ErrorCode::MethodNotFound.into())
    }

    async fn try_get_object_before_version(
        &self,
        _: ObjectID,
        _: SequenceNumber,
    ) -> RpcResult<SuiPastObjectResponse> {
        Err(jsonrpsee::types::error::ErrorCode::MethodNotFound.into())
    }

    async fn try_multi_get_past_objects(
        &self,
        _past_objects: Vec<SuiGetPastObjectRequest>,
        _options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>> {
        Err(jsonrpsee::types::error::ErrorCode::MethodNotFound.into())
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<BigInt<u64>> {
        let checkpoint = self.get_latest_checkpoint().await?;
        Ok(BigInt::from(checkpoint.sequence_number))
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> RpcResult<Checkpoint> {
        self.get_checkpoint(id).await.map_err(Into::into)
    }

    async fn get_checkpoints(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> RpcResult<CheckpointPage> {
        let cursor = cursor.map(BigInt::into_inner);
        let limit = sui_json_rpc_api::validate_limit(
            limit,
            sui_json_rpc_api::QUERY_MAX_RESULT_LIMIT_CHECKPOINTS,
        )
        .map_err(SuiRpcInputError::from)?;

        let mut checkpoints = self
            .inner
            .get_checkpoints(cursor, limit + 1, descending_order)
            .await?;

        let has_next_page = checkpoints.len() > limit;
        checkpoints.truncate(limit);

        let next_cursor = checkpoints.last().map(|d| d.sequence_number.into());

        Ok(CheckpointPage {
            data: checkpoints,
            next_cursor,
            has_next_page,
        })
    }

    async fn get_events(&self, transaction_digest: TransactionDigest) -> RpcResult<Vec<SuiEvent>> {
        self.inner
            .get_transaction_events(transaction_digest)
            .await
            .map_err(Into::into)
    }

    async fn get_protocol_config(
        &self,
        version: Option<BigInt<u64>>,
    ) -> RpcResult<ProtocolConfigResponse> {
        let chain = self.get_chain_identifier().await?.chain();
        let version = if let Some(version) = version {
            (*version).into()
        } else {
            let latest_epoch = self.inner.get_latest_epoch_info_from_db().await?;
            (latest_epoch.protocol_version as u64).into()
        };

        ProtocolConfig::get_for_version_if_supported(version, chain)
            .ok_or(SuiRpcInputError::ProtocolVersionUnsupported(
                ProtocolVersion::MIN.as_u64(),
                ProtocolVersion::MAX.as_u64(),
            ))
            .map_err(Into::into)
            .map(ProtocolConfigResponse::from)
    }

    async fn get_chain_identifier(&self) -> RpcResult<String> {
        self.get_chain_identifier().await.map(|id| id.to_string())
    }

    async fn verify_zklogin_signature(
        &self,
        _bytes: String,
        _signature: String,
        _intent_scope: ZkLoginIntentScope,
        _author: SuiAddress,
    ) -> RpcResult<ZkLoginVerifyResult> {
        todo!()
    }
}

impl SuiRpcModule for ReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc_api::ReadApiOpenRpc::module_doc()
    }
}

async fn object_read_to_object_response(
    indexer_reader: &IndexerReader,
    object_read: ObjectRead,
    options: SuiObjectDataOptions,
) -> RpcResult<SuiObjectResponse> {
    match object_read {
        ObjectRead::NotExists(id) => Ok(SuiObjectResponse::new_with_error(
            SuiObjectResponseError::NotExists { object_id: id },
        )),
        ObjectRead::Exists(object_ref, o, layout) => {
            let mut display_fields = None;
            if options.show_display {
                match indexer_reader.get_display_fields(&o, &layout).await {
                    Ok(rendered_fields) => display_fields = Some(rendered_fields),
                    Err(e) => {
                        return Ok(SuiObjectResponse::new(
                            Some(
                                (object_ref, o, layout, options, None)
                                    .try_into()
                                    .map_err(IndexerError::from)?,
                            ),
                            Some(SuiObjectResponseError::DisplayError {
                                error: e.to_string(),
                            }),
                        ));
                    }
                }
            }
            Ok(SuiObjectResponse::new_with_data(
                (object_ref, o, layout, options, display_fields)
                    .try_into()
                    .map_err(IndexerError::from)?,
            ))
        }
        ObjectRead::Deleted((object_id, version, digest)) => Ok(SuiObjectResponse::new_with_error(
            SuiObjectResponseError::Deleted {
                object_id,
                version,
                digest,
            },
        )),
    }
}
