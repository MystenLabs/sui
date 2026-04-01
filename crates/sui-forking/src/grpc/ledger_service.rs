// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `LedgerService` implementation for forked object reads.
//!
//! Currently implements `get_object` and `batch_get_objects`. The remaining
//! ledger APIs stay unimplemented until the supported server surface is
//! widened.

use std::sync::Arc;

use rand::rngs::OsRng;
use simulacrum::{Simulacrum, SimulatorStore};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::{
    ledger_service_server::LedgerService, BatchGetObjectsRequest, BatchGetObjectsResponse,
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, GetCheckpointRequest,
    GetCheckpointResponse, GetEpochRequest, GetEpochResponse, GetObjectRequest, GetObjectResponse,
    GetObjectResult, GetServiceInfoRequest, GetServiceInfoResponse, GetTransactionRequest,
    GetTransactionResponse,
};
use sui_rpc_api::grpc::v2::ledger_service::validate_get_object_requests;
use sui_rpc_api::{ObjectNotFoundError, RpcError};
use tokio::sync::RwLock;

const MAX_BATCH_REQUESTS: usize = 1000;

/// Minimal LedgerService implementation for object fetching over a forked store.
pub(crate) struct ForkingLedgerService<S: SimulatorStore> {
    simulacrum: Arc<RwLock<Simulacrum<OsRng, S>>>,
}

impl<S: SimulatorStore> ForkingLedgerService<S> {
    pub(crate) fn new(simulacrum: Arc<RwLock<Simulacrum<OsRng, S>>>) -> Self {
        Self { simulacrum }
    }
}

#[tonic::async_trait]
impl<S> LedgerService for ForkingLedgerService<S>
where
    S: SimulatorStore + Send + Sync + 'static,
{
    async fn get_service_info(
        &self,
        _request: tonic::Request<GetServiceInfoRequest>,
    ) -> Result<tonic::Response<GetServiceInfoResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "get_service_info is not yet implemented",
        ))
    }

    async fn get_object(
        &self,
        request: tonic::Request<GetObjectRequest>,
    ) -> Result<tonic::Response<GetObjectResponse>, tonic::Status> {
        let GetObjectRequest {
            object_id,
            version,
            read_mask,
            ..
        } = request.into_inner();

        let (requests, read_mask) =
            validate_get_object_requests(vec![(object_id, version)], read_mask)
                .map_err(tonic::Status::from)?;
        let (object_id, version) = requests[0];
        let object = self
            .get_object_impl(object_id.into(), version)
            .await
            .map_err(tonic::Status::from)?;

        let mut proto_object = sui_rpc::proto::sui::rpc::v2::Object::default();
        proto_object.merge(&object, &read_mask);
        Ok(tonic::Response::new(GetObjectResponse::new(proto_object)))
    }

    async fn batch_get_objects(
        &self,
        request: tonic::Request<BatchGetObjectsRequest>,
    ) -> Result<tonic::Response<BatchGetObjectsResponse>, tonic::Status> {
        let BatchGetObjectsRequest {
            requests,
            read_mask,
            ..
        } = request.into_inner();
        if requests.len() > MAX_BATCH_REQUESTS {
            return Err(tonic::Status::invalid_argument(format!(
                "number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"
            )));
        }

        let requests = requests
            .into_iter()
            .map(|request| (request.object_id, request.version))
            .collect();
        let (requests, read_mask) =
            validate_get_object_requests(requests, read_mask).map_err(tonic::Status::from)?;

        let mut results = Vec::with_capacity(requests.len());
        for (object_id, version) in requests {
            let result = match self.get_object_impl(object_id.into(), version).await {
                Ok(object) => {
                    let mut proto_object = sui_rpc::proto::sui::rpc::v2::Object::default();
                    proto_object.merge(&object, &read_mask);
                    GetObjectResult::new_object(proto_object)
                }
                Err(error) => GetObjectResult::new_error(error.into_status_proto()),
            };
            results.push(result);
        }

        Ok(tonic::Response::new(BatchGetObjectsResponse::new(results)))
    }

    async fn get_transaction(
        &self,
        _request: tonic::Request<GetTransactionRequest>,
    ) -> Result<tonic::Response<GetTransactionResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "get_transaction is not yet implemented",
        ))
    }

    async fn batch_get_transactions(
        &self,
        _request: tonic::Request<BatchGetTransactionsRequest>,
    ) -> Result<tonic::Response<BatchGetTransactionsResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "batch_get_transactions is not yet implemented",
        ))
    }

    async fn get_checkpoint(
        &self,
        _request: tonic::Request<GetCheckpointRequest>,
    ) -> Result<tonic::Response<GetCheckpointResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "get_checkpoint is not yet implemented",
        ))
    }

    async fn get_epoch(
        &self,
        _request: tonic::Request<GetEpochRequest>,
    ) -> Result<tonic::Response<GetEpochResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "get_epoch is not yet implemented",
        ))
    }
}

impl<S> ForkingLedgerService<S>
where
    S: SimulatorStore + Send + Sync + 'static,
{
    async fn get_object_impl(
        &self,
        object_id: sui_types::base_types::ObjectID,
        version: Option<u64>,
    ) -> Result<sui_types::object::Object, RpcError> {
        let simulacrum = self.simulacrum.read().await;
        let store = simulacrum.store();
        let object = if let Some(version) = version {
            store.get_object_at_version(&object_id, version.into())
        } else {
            SimulatorStore::get_object(store, &object_id)
        };

        object.ok_or_else(|| {
            if let Some(version) = version {
                ObjectNotFoundError::new_with_version(object_id.into(), version).into()
            } else {
                ObjectNotFoundError::new(object_id.into()).into()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroUsize, sync::Arc};

    use super::*;
    use crate::ServiceStore;
    use crate::test_utils::{TestForkingStore, forking_store, test_object};
    use forking_data_store::{ObjectKey, ObjectStoreWriter, VersionQuery};
    use sui_rpc::proto::sui::rpc::v2::{BatchGetObjectsRequest, GetObjectRequest};
    use sui_swarm_config::network_config_builder::ConfigBuilder;
    use sui_types::{
        base_types::{ObjectID, SuiAddress},
        digests::get_mainnet_chain_identifier,
    };
    use tempfile::tempdir;

    type TestServiceStore = ServiceStore<TestForkingStore, TestForkingStore>;

    fn build_simulacrum(
        store: TestServiceStore,
    ) -> Arc<RwLock<Simulacrum<OsRng, TestServiceStore>>> {
        let mut rng = OsRng;
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .with_chain_start_timestamp_ms(0)
            .deterministic_committee_size(NonZeroUsize::MIN)
            .build();
        Arc::new(RwLock::new(Simulacrum::new_with_network_config_store(
            &config, OsRng, store,
        )))
    }

    #[tokio::test]
    async fn get_object_returns_locally_available_object() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();
        let object_id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();
        let historical_store = forking_store(historical_dir.path(), &chain_id);
        let local_store = forking_store(local_dir.path(), &chain_id);
        let object_id_string = object_id.to_string();
        local_store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::Version(22),
                },
                test_object(object_id, owner, 22),
                22,
            )
            .unwrap();
        let store = ServiceStore::new(100, historical_store, local_store);

        let service = ForkingLedgerService::new(build_simulacrum(store));
        let mut request = GetObjectRequest::default();
        request.object_id = Some(object_id_string.clone());
        request.version = Some(22);
        let response = <ForkingLedgerService<_> as LedgerService>::get_object(
            &service,
            tonic::Request::new(request),
        )
        .await
        .unwrap()
        .into_inner();

        assert_eq!(response.object().version, Some(22));
        assert_eq!(
            response.object().object_id.as_deref(),
            Some(object_id_string.as_str())
        );
    }

    #[tokio::test]
    async fn batch_get_objects_returns_objects_and_per_entry_errors() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();
        let object_id = ObjectID::random();
        let missing_id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();
        let object_id_string = object_id.to_string();
        let missing_id_string = missing_id.to_string();
        let historical_store = forking_store(historical_dir.path(), &chain_id);
        historical_store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::AtCheckpoint(100),
                },
                test_object(object_id, owner, 10),
                10,
            )
            .unwrap();
        let store = ServiceStore::new(
            100,
            historical_store,
            forking_store(local_dir.path(), &chain_id),
        );

        let service = ForkingLedgerService::new(build_simulacrum(store));
        let mut present_request = GetObjectRequest::default();
        present_request.object_id = Some(object_id_string.clone());
        let mut missing_request = GetObjectRequest::default();
        missing_request.object_id = Some(missing_id_string);
        let mut request = BatchGetObjectsRequest::default();
        request.requests = vec![present_request, missing_request];
        let response = <ForkingLedgerService<_> as LedgerService>::batch_get_objects(
            &service,
            tonic::Request::new(request),
        )
        .await
        .unwrap()
        .into_inner();

        assert_eq!(
            response.objects[0].object().object_id.as_deref(),
            Some(object_id_string.as_str())
        );
        assert!(response.objects[1].error().message.contains("not found"));
    }
}
