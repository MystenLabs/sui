// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::dataloader::DataLoader;
use move_core_types::account_address::AccountAddress;
use prost_types::FieldMask;
use sui_package_resolver::Package;
use sui_package_resolver::PackageStore;
use sui_package_resolver::PackageStoreWithLruCache;
use sui_package_resolver::Result;
use sui_package_resolver::error::Error;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

use crate::ledger_grpc_reader::ChunkedLoader;
use crate::ledger_grpc_reader::LedgerGrpcReader;

const STORE: &str = "Ledger gRPC";

pub type PackageCache = PackageStoreWithLruCache<GrpcPackageStore>;

/// Package store that reads packages from the ledger gRPC service (kv-rpc or fullnode) by their
/// storage ID.
pub struct GrpcPackageStore(Arc<DataLoader<LedgerGrpcReader>>);

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct PackageKey(AccountAddress);

impl GrpcPackageStore {
    pub fn new(reader: &LedgerGrpcReader) -> Self {
        Self(Arc::new(reader.as_data_loader()))
    }
}

#[async_trait::async_trait]
impl PackageStore for GrpcPackageStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let Self(loader) = self;
        let Some(package) = loader.load_one(PackageKey(id)).await? else {
            return Err(Error::PackageNotFound(id));
        };

        Ok(package)
    }
}

#[async_trait::async_trait]
impl ChunkedLoader<PackageKey> for LedgerGrpcReader {
    type Value = Arc<Package>;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        self.max_batch_get_objects()
    }

    async fn load_chunk(&self, keys: &[PackageKey]) -> Result<HashMap<PackageKey, Arc<Package>>> {
        let mut id_to_package = HashMap::new();
        if keys.is_empty() {
            return Ok(id_to_package);
        }

        let requests = keys
            .iter()
            .map(|PackageKey(id)| proto::GetObjectRequest::new(&ObjectID::from(*id).into()))
            .collect();

        let mut request = proto::BatchGetObjectsRequest::default();
        request.requests = requests;
        request.read_mask = Some(FieldMask::from_paths(["bcs"]));

        let batch_response = self.batch_get_objects(request).await.map_err(store_err)?;

        for obj_result in batch_response.objects {
            let object = match obj_result.result {
                Some(proto::get_object_result::Result::Object(object)) => object,

                Some(proto::get_object_result::Result::Error(status))
                    if status.code == tonic::Code::NotFound as i32 =>
                {
                    continue;
                }

                Some(proto::get_object_result::Result::Error(status)) => {
                    return Err(store_err(format!(
                        "{:?}: {}",
                        tonic::Code::from(status.code),
                        status.message
                    )));
                }

                _ => {
                    return Err(store_err("Empty or unrecognized result in batch response"));
                }
            };

            let object: Object = object
                .bcs
                .as_ref()
                .ok_or_else(|| store_err("Missing bcs in object"))?
                .deserialize()
                .map_err(|e| store_err(format!("Failed to deserialize object: {e}")))?;
            // Any object ID can be requested, e.g. from a user-supplied type. A non-package object
            // is a miss for its own key, not an error for every key in the shared batch.
            let Some(move_package) = object.data.try_as_package() else {
                continue;
            };

            let package = Package::read_from_package(move_package)?;
            id_to_package.insert(PackageKey(*move_package.id()), Arc::new(package));
        }

        Ok(id_to_package)
    }
}

fn store_err(e: impl ToString) -> Error {
    Error::Store {
        store: STORE,
        error: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use sui_rpc::proto::google::rpc::Status as StatusProto;
    use sui_rpc::proto::sui::rpc::v2::GetObjectResult;

    use super::*;
    use crate::ledger_grpc_reader::test_support::mock_reader;

    fn error_result(code: tonic::Code, message: &str) -> GetObjectResult {
        GetObjectResult::new_error(StatusProto {
            code: code as i32,
            message: message.to_owned(),
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn per_object_not_found_is_package_not_found() {
        let (reader, mock, _server) = mock_reader().await;
        mock.set_object_results(vec![error_result(
            tonic::Code::NotFound,
            "object not found",
        )]);

        let store = GrpcPackageStore::new(&reader);
        let err = store.fetch(AccountAddress::TWO).await.unwrap_err();
        assert!(matches!(err, Error::PackageNotFound(_)), "{err:?}");
    }

    #[tokio::test]
    async fn per_object_backend_error_is_store_error() {
        let (reader, mock, _server) = mock_reader().await;
        mock.set_object_results(vec![error_result(tonic::Code::Internal, "boom")]);

        let store = GrpcPackageStore::new(&reader);
        let err = store.fetch(AccountAddress::TWO).await.unwrap_err();
        assert!(
            matches!(
                &err,
                Error::Store { store, error }
                    if *store == STORE && error.contains("Internal") && error.contains("boom")
            ),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn per_object_empty_result_is_store_error() {
        let (reader, mock, _server) = mock_reader().await;
        mock.set_object_results(vec![GetObjectResult::default()]);

        let store = GrpcPackageStore::new(&reader);
        let err = store.fetch(AccountAddress::TWO).await.unwrap_err();
        assert!(matches!(&err, Error::Store { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn non_package_object_is_package_not_found() {
        let (reader, mock, _server) = mock_reader().await;
        let object = Object::immutable_with_id_for_testing(ObjectID::from(AccountAddress::TWO));
        let mut proto_object = proto::Object::default();
        proto_object.bcs = Some(proto::Bcs::serialize(&object).unwrap());
        mock.set_object_results(vec![GetObjectResult::new_object(proto_object)]);

        let store = GrpcPackageStore::new(&reader);
        let err = store.fetch(AccountAddress::TWO).await.unwrap_err();
        assert!(matches!(err, Error::PackageNotFound(_)), "{err:?}");
    }
}
