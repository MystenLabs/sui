// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::{GetObjectRequest, GetObjectResponse, GetObjectResult};
use sui_rpc_api::{
    ObjectNotFoundError, RpcError, grpc::v2::ledger_service::validate_get_object_requests,
};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::storage::ObjectKey;

use crate::PackageResolver;
use crate::render::object_to_response;

pub const MAX_BATCH_REQUESTS: usize = 1000;
pub const MAX_UNVERSIONED_BATCH_REQUESTS: usize = 50;
pub(crate) async fn get_object(
    mut client: BigTableClient,
    GetObjectRequest {
        object_id,
        version,
        read_mask,
        ..
    }: GetObjectRequest,
    resolver: &PackageResolver,
) -> Result<GetObjectResponse, RpcError> {
    let (requests, read_mask) =
        validate_get_object_requests(vec![(object_id, version)], read_mask)?;
    let (object_id, version) = requests[0];
    let object = match version {
        Some(version) => client
            .get_objects(&[ObjectKey(object_id.into(), version.into())])
            .await?
            .pop()
            .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?,
        None => client
            .get_latest_object(&object_id.into())
            .await?
            .ok_or_else(|| ObjectNotFoundError::new(object_id))?,
    };
    let message = object_to_response(&object, &read_mask, resolver).await;
    Ok(GetObjectResponse::new(message))
}

pub(crate) async fn batch_get_objects(
    client: BigTableClient,
    BatchGetObjectsRequest {
        requests,
        read_mask,
        ..
    }: BatchGetObjectsRequest,
    resolver: &PackageResolver,
) -> Result<BatchGetObjectsResponse, RpcError> {
    if requests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }

    let unversioned_count = requests.iter().filter(|r| r.version.is_none()).count();
    if unversioned_count > MAX_UNVERSIONED_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!(
                "number of unversioned batch requests exceeds limit of {MAX_UNVERSIONED_BATCH_REQUESTS}"
            ),
        ));
    }

    let requests = requests
        .into_iter()
        .map(|req| (req.object_id, req.version))
        .collect();
    let (requests, read_mask) = validate_get_object_requests(requests, read_mask)?;

    let mut exact_object_keys = Vec::new();
    let mut unversioned_object_ids = Vec::new();
    for (address, version) in &requests {
        let object_id: ObjectID = (*address).into();
        match version {
            Some(version) => {
                let sequence_number: SequenceNumber = (*version).into();
                exact_object_keys.push(ObjectKey(object_id, sequence_number));
            }
            None => {
                unversioned_object_ids.push(object_id);
            }
        }
    }

    exact_object_keys.sort();
    exact_object_keys.dedup();
    unversioned_object_ids.sort();
    unversioned_object_ids.dedup();

    let exact_future = async {
        if exact_object_keys.is_empty() {
            Ok(HashMap::new())
        } else {
            let mut client = client.clone();
            let objects = client.get_objects(&exact_object_keys).await?;
            Ok(objects
                .into_iter()
                .map(|obj| ((obj.id(), obj.version()), obj))
                .collect())
        }
    };

    let unversioned_future = async {
        let unversioned_futures = unversioned_object_ids.into_iter().map(|object_id| {
            let mut client = client.clone();
            async move {
                let object = client.get_latest_object(&object_id).await?;
                Ok::<_, RpcError>((object_id, object))
            }
        });
        let unversioned_results = futures::future::join_all(unversioned_futures).await;

        let mut unversioned_objects = HashMap::new();
        for result in unversioned_results {
            let (object_id, object) = result?;
            if let Some(object) = object {
                unversioned_objects.insert(object_id, object);
            }
        }
        Ok::<_, RpcError>(unversioned_objects)
    };

    let (exact_objects, unversioned_objects) =
        tokio::try_join!(exact_future, unversioned_future)?;

    let mut objects = Vec::with_capacity(requests.len());
    for (address, version) in requests {
        let object_id: ObjectID = address.into();
        match version {
            Some(version) => {
                let sequence_number: SequenceNumber = version.into();
                if let Some(object) = exact_objects.get(&(object_id, sequence_number)) {
                    let message = object_to_response(object, &read_mask, resolver).await;
                    objects.push(GetObjectResult::new_object(message));
                } else {
                    let err: RpcError =
                        ObjectNotFoundError::new_with_version(address, version).into();
                    objects.push(GetObjectResult::new_error(err.into_status_proto()));
                }
            }
            None => {
                if let Some(object) = unversioned_objects.get(&object_id) {
                    let message = object_to_response(object, &read_mask, resolver).await;
                    objects.push(GetObjectResult::new_object(message));
                } else {
                    let err: RpcError = ObjectNotFoundError::new(address).into();
                    objects.push(GetObjectResult::new_error(err.into_status_proto()));
                }
            }
        }
    }
    Ok(BatchGetObjectsResponse::new(objects))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_kvstore::BigTableClient as InnerBigTableClient;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_package_resolver::PackageStore;
    use sui_package_resolver::Resolver;
    use sui_types::base_types::ObjectID;

    use super::*;
    use crate::package_store::BigTablePackageStore;

    #[tokio::test]
    async fn test_batch_get_objects_limit_exceeded() {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let client = InnerBigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        let package_store: Arc<dyn PackageStore> =
            Arc::new(BigTablePackageStore::new(client.clone()));
        let resolver = Arc::new(Resolver::new(package_store));

        let requests = (0..MAX_BATCH_REQUESTS + 1)
            .map(|_| {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(ObjectID::random().to_canonical_string(true));
                req.version = Some(1);
                req
            })
            .collect();

        let mut req = BatchGetObjectsRequest::default();
        req.requests = requests;

        let err = batch_get_objects(client, req, &resolver).await.unwrap_err();
        let status: tonic::Status = err.into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);

        server.abort();
    }

    #[tokio::test]
    async fn test_batch_get_objects_unversioned_limit_exceeded() {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let client = InnerBigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        let package_store: Arc<dyn PackageStore> =
            Arc::new(BigTablePackageStore::new(client.clone()));
        let resolver = Arc::new(Resolver::new(package_store));

        let requests = (0..MAX_UNVERSIONED_BATCH_REQUESTS + 1)
            .map(|_| {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(ObjectID::random().to_canonical_string(true));
                req.version = None;
                req
            })
            .collect();

        let mut req = BatchGetObjectsRequest::default();
        req.requests = requests;

        let err = batch_get_objects(client, req, &resolver).await.unwrap_err();
        let status: tonic::Status = err.into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);

        server.abort();
    }
}
