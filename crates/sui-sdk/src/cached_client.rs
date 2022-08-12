// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use move_binary_format::access::ModuleAccess;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use tokio::sync::RwLock;

use sui_json_rpc_types::{
    GatewayTxSeqNumber, GetObjectDataResponse, GetRawObjectDataResponse, SuiObjectInfo,
    SuiParsedObject, SuiTransactionResponse,
};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::messages::Transaction;
use sui_types::object::{Data, Object, ObjectFormatOptions};

use crate::{ClientCache, QuorumDriver, QuorumDriverImpl, ReadApi, ReadApiImpl, ResolverWrapper};

pub(crate) struct CachedReadApi {
    pub read_api: ReadApiImpl,
    pub state: Arc<RwLock<ClientCache>>,
    pub module_cache: SyncModuleCache<ResolverWrapper<ClientCache>>,
}

#[async_trait]
impl ReadApi for CachedReadApi {
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        self.read_api.get_objects_owned_by_address(address).await
    }

    async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        self.read_api.get_objects_owned_by_object(object_id).await
    }

    async fn get_parsed_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetObjectDataResponse> {
        let response = self.get_object(object_id).await?;
        self.parse_object_response(response).await
    }
    async fn get_object(&self, object_id: ObjectID) -> anyhow::Result<GetRawObjectDataResponse> {
        let response = self.state.read().await.get_object(object_id).cloned();
        Ok(if let Some(response) = response {
            response
        } else {
            let response = self.read_api.get_object(object_id).await?;
            self.state.write().await.update_object(response.clone());
            response
        })
    }

    async fn get_total_transaction_number(&self) -> anyhow::Result<u64> {
        self.read_api.get_total_transaction_number().await
    }

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> anyhow::Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        self.read_api.get_transactions_in_range(start, end).await
    }

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> anyhow::Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        self.read_api.get_recent_transactions(count).await
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> anyhow::Result<SuiTransactionResponse> {
        self.read_api.get_transaction(digest).await
    }
}

impl CachedReadApi {
    async fn parse_object_response(
        &self,
        object: GetRawObjectDataResponse,
    ) -> Result<GetObjectDataResponse, anyhow::Error> {
        Ok(match object {
            GetRawObjectDataResponse::Exists(o) => {
                let object: Object = o.try_into()?;
                let layout = match &object.data {
                    Data::Move(object) => {
                        self.load_object_transitive_deps(&object.type_).await?;
                        Some(
                            object
                                .get_layout(ObjectFormatOptions::default(), &self.module_cache)?,
                        )
                    }
                    Data::Package(_) => None,
                };
                GetObjectDataResponse::Exists(SuiParsedObject::try_from(object, layout)?)
            }
            GetRawObjectDataResponse::NotExists(id) => GetObjectDataResponse::NotExists(id),
            GetRawObjectDataResponse::Deleted(oref) => GetObjectDataResponse::Deleted(oref),
        })
    }

    // this function over-approximates
    // it loads all modules used in the type declaration
    // and then all of their dependencies.
    // To be exact, it would need to look at the field layout for each type used, but this will
    // be complicated with generics. The extra loading here is hopefully insignificant
    async fn load_object_transitive_deps(
        &self,
        struct_tag: &StructTag,
    ) -> Result<(), anyhow::Error> {
        fn used_packages(packages: &mut Vec<ObjectID>, type_: &TypeTag) {
            match type_ {
                TypeTag::Bool
                | TypeTag::U8
                | TypeTag::U64
                | TypeTag::U128
                | TypeTag::Address
                | TypeTag::Signer => (),
                TypeTag::Vector(inner) => used_packages(packages, inner),
                TypeTag::Struct(StructTag {
                    address,
                    type_params,
                    ..
                }) => {
                    packages.push((*address).into());
                    for t in type_params {
                        used_packages(packages, t)
                    }
                }
            }
        }
        let StructTag {
            address,
            type_params,
            ..
        } = struct_tag;
        let mut queue = vec![(*address).into()];
        for t in type_params {
            used_packages(&mut queue, t)
        }

        let mut seen: HashSet<ObjectID> = HashSet::new();
        while let Some(cur) = queue.pop() {
            if seen.contains(&cur) {
                continue;
            }
            let obj = self.get_object(cur).await?;
            let obj: Object = obj.into_object()?.try_into()?;
            let package = match &obj.data {
                Data::Move(_) => {
                    debug_assert!(false, "{cur} should be a package, not a move object");
                    continue;
                }
                Data::Package(package) => package,
            };
            let modules = package
                .serialized_module_map()
                .keys()
                .map(|name| package.deserialize_module(&Identifier::new(name.clone()).unwrap()))
                .collect::<Result<Vec<_>, _>>()?;
            for module in modules {
                let self_package_idx = module
                    .module_handle_at(module.self_module_handle_idx)
                    .address;
                let self_package = *module.address_identifier_at(self_package_idx);
                seen.insert(self_package.into());
                for handle in &module.module_handles {
                    let dep_package = *module.address_identifier_at(handle.address);
                    queue.push(dep_package.into());
                }
            }
        }
        Ok(())
    }
}

pub(crate) struct CachedQuorumDriver {
    pub quorum_driver: QuorumDriverImpl,
    pub state: Arc<RwLock<ClientCache>>,
}

#[async_trait]
impl QuorumDriver for CachedQuorumDriver {
    async fn execute_transaction(&self, tx: Transaction) -> anyhow::Result<SuiTransactionResponse> {
        let response = self.quorum_driver.execute_transaction(tx).await?;
        let mut all_changes = response
            .effects
            .mutated
            .iter()
            .map(|oref| oref.reference.clone())
            .collect::<Vec<_>>();
        all_changes.extend(response.effects.deleted.clone());
        let all_changes = all_changes
            .iter()
            .map(|oref| oref.to_object_ref())
            .collect::<Vec<_>>();
        self.state.write().await.update_refs(all_changes);
        Ok(response)
    }
}
