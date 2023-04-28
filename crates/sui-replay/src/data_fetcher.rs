// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::LocalExecError;
use async_trait::async_trait;
use futures::future::join_all;
use sui_json_rpc::api::QUERY_MAX_RESULT_LIMIT;
use sui_json_rpc_types::SuiGetPastObjectRequest;
use sui_json_rpc_types::SuiObjectData;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_json_rpc_types::SuiObjectResponse;
use sui_json_rpc_types::SuiPastObjectResponse;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, SequenceNumber, VersionNumber};
use sui_types::digests::TransactionDigest;
use sui_types::object::Object;
use tracing::error;

/// This trait defines the interfaces for fetching data from some local or remote store
#[async_trait]
pub(crate) trait DataFetcher {
    #![allow(implied_bounds_entailment)]
    /// Fetch the specified versions of objects
    async fn multi_get_versioned(
        &self,
        objects: &[(ObjectID, SequenceNumber)],
    ) -> Result<Vec<Object>, LocalExecError>;

    /// Fetch the latest versions of objects
    async fn multi_get_latest(&self, objects: &[ObjectID]) -> Result<Vec<Object>, LocalExecError>;

    /// Fetch the TXs for this checkpoint
    async fn get_checkpoint_txs(&self, id: u64) -> Result<Vec<TransactionDigest>, LocalExecError>;

    /// Fetch the transaction info for a given transaction digest
    async fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<SuiTransactionBlockResponse, LocalExecError>;

    async fn get_loaded_child_objects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, LocalExecError>;
}

pub struct RemoteFetcher {
    /// This is used to download items not in store
    pub rpc_client: SuiClient,
}

#[async_trait]
impl DataFetcher for RemoteFetcher {
    #![allow(implied_bounds_entailment)]
    async fn multi_get_versioned(
        &self,
        objects: &[(ObjectID, VersionNumber)],
    ) -> Result<Vec<Object>, LocalExecError> {
        let options = SuiObjectDataOptions::bcs_lossless();

        let objs: Vec<_> = objects
            .iter()
            .map(|(object_id, version)| SuiGetPastObjectRequest {
                object_id: *object_id,
                version: *version,
            })
            .collect();

        let objectsx = objs.chunks(*QUERY_MAX_RESULT_LIMIT).map(|q| {
            self.rpc_client
                .read_api()
                .try_multi_get_parsed_past_object(q.to_vec(), options.clone())
        });

        join_all(objectsx)
            .await
            .into_iter()
            .collect::<Result<Vec<Vec<_>>, _>>()
            .map_err(LocalExecError::from)?
            .iter()
            .flatten()
            .map(|q| convert_past_obj_response(q.clone()))
            .collect::<Result<Vec<_>, _>>()
    }

    async fn multi_get_latest(&self, objects: &[ObjectID]) -> Result<Vec<Object>, LocalExecError> {
        let options = SuiObjectDataOptions::bcs_lossless();

        let objectsx = objects.chunks(*QUERY_MAX_RESULT_LIMIT).map(|q| {
            self.rpc_client
                .read_api()
                .multi_get_object_with_options(q.to_vec(), options.clone())
        });

        join_all(objectsx)
            .await
            .into_iter()
            .collect::<Result<Vec<Vec<_>>, _>>()
            .map_err(LocalExecError::from)?
            .iter()
            .flatten()
            .map(obj_from_sui_obj_response)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn get_checkpoint_txs(&self, id: u64) -> Result<Vec<TransactionDigest>, LocalExecError> {
        Ok(self
            .rpc_client
            .read_api()
            .get_checkpoint(id.into())
            .await
            .map_err(|q| LocalExecError::SuiRpcError { err: q.to_string() })?
            .transactions)
    }

    async fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<SuiTransactionBlockResponse, LocalExecError> {
        let tx_fetch_opts = SuiTransactionBlockResponseOptions::full_content();

        self.rpc_client
            .read_api()
            .get_transaction_with_options(*tx_digest, tx_fetch_opts)
            .await
            .map_err(LocalExecError::from)
    }

    async fn get_loaded_child_objects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, LocalExecError> {
        let loaded_child_objs = match self
            .rpc_client
            .read_api()
            .get_loaded_child_objects(*tx_digest)
            .await
        {
            Ok(objs) => objs,
            Err(e) => {
                error!("Error getting dynamic fields loaded objects: {}. This RPC server might not support this feature yet", e);
                return Err(LocalExecError::UnableToGetDynamicFieldLoadedObjects {
                    rpc_err: e.to_string(),
                });
            }
        };

        // Fetch the refs
        Ok(loaded_child_objs
            .loaded_child_objects
            .iter()
            .map(|obj| (obj.object_id(), obj.sequence_number()))
            .collect::<Vec<_>>())
    }
}

fn convert_past_obj_response(resp: SuiPastObjectResponse) -> Result<Object, LocalExecError> {
    match resp {
        SuiPastObjectResponse::VersionFound(o) => obj_from_sui_obj_data(&o),
        SuiPastObjectResponse::ObjectDeleted(r) => Err(LocalExecError::ObjectDeleted {
            id: r.object_id,
            version: r.version,
            digest: r.digest,
        }),
        SuiPastObjectResponse::ObjectNotExists(id) => Err(LocalExecError::ObjectNotExist { id }),
        SuiPastObjectResponse::VersionNotFound(id, version) => {
            Err(LocalExecError::ObjectVersionNotFound { id, version })
        }
        SuiPastObjectResponse::VersionTooHigh {
            object_id,
            asked_version,
            latest_version,
        } => Err(LocalExecError::ObjectVersionTooHigh {
            id: object_id,
            asked_version,
            latest_version,
        }),
    }
}

fn obj_from_sui_obj_response(o: &SuiObjectResponse) -> Result<Object, LocalExecError> {
    let o = o.object().map_err(LocalExecError::from)?.clone();
    obj_from_sui_obj_data(&o)
}

fn obj_from_sui_obj_data(o: &SuiObjectData) -> Result<Object, LocalExecError> {
    match TryInto::<Object>::try_into(o.clone()) {
        Ok(obj) => Ok(obj),
        Err(e) => Err(e.into()),
    }
}
