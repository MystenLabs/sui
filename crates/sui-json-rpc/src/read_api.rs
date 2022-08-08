// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::RpcFullNodeReadApiServer;
use crate::api::RpcReadApiServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee_core::server::rpc_module::RpcModule;
use move_binary_format::normalized::{Module as NormalizedModule, Type};
use move_core_types::identifier::Identifier;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_core::gateway_state::GatewayTxSeqNumber;
use sui_json_rpc_types::{
    GetObjectDataResponse, MoveFunctionArgType, ObjectValueKind, SuiMoveNormalizedFunction,
    SuiMoveNormalizedModule, SuiMoveNormalizedStruct, SuiObjectInfo, SuiTransactionEffects,
    TransactionEffectsResponse,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::move_package::normalize_modules;
use sui_types::object::{Data, ObjectRead, Owner};

// An implementation of the read portion of the Gateway JSON-RPC interface intended for use in
// Fullnodes.
pub struct ReadApi {
    pub state: Arc<AuthorityState>,
}

pub struct FullNodeApi {
    pub state: Arc<AuthorityState>,
}

impl FullNodeApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

impl ReadApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl RpcReadApiServer for ReadApi {
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> RpcResult<Vec<SuiObjectInfo>> {
        Ok(self
            .state
            .get_owner_objects(Owner::AddressOwner(address))
            .map_err(|e| anyhow!("{e}"))?
            .into_iter()
            .map(SuiObjectInfo::from)
            .collect())
    }

    async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> RpcResult<Vec<SuiObjectInfo>> {
        Ok(self
            .state
            .get_owner_objects(Owner::ObjectOwner(object_id.into()))
            .map_err(|e| anyhow!("{e}"))?
            .into_iter()
            .map(SuiObjectInfo::from)
            .collect())
    }

    async fn get_object(&self, object_id: ObjectID) -> RpcResult<GetObjectDataResponse> {
        Ok(self
            .state
            .get_object_read(&object_id)
            .await
            .map_err(|e| anyhow!("{e}"))?
            .try_into()?)
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.state.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_in_range(start, end)?)
    }

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_recent_transactions(count)?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse> {
        let (cert, effects) = self.state.get_transaction(digest).await?;
        Ok(TransactionEffectsResponse {
            certificate: cert.try_into()?,
            effects: SuiTransactionEffects::try_from(effects, &self.state.module_cache)?,
            timestamp_ms: self.state.get_timestamp_ms(&digest).await?,
        })
    }
}

impl SuiRpcModule for ReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::RpcReadApiOpenRpc::module_doc()
    }
}

#[async_trait]
impl RpcFullNodeReadApiServer for FullNodeApi {
    async fn get_normalized_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> RpcResult<BTreeMap<String, SuiMoveNormalizedModule>> {
        let modules = get_move_modules_by_package(self, package).await?;
        Ok(modules
            .into_iter()
            .map(|(name, module)| (name, module.into()))
            .collect::<BTreeMap<String, SuiMoveNormalizedModule>>())
    }

    async fn get_normalized_move_module(
        &self,
        package: ObjectID,
        module_name: String,
    ) -> RpcResult<SuiMoveNormalizedModule> {
        let module = get_move_module(self, package, module_name).await?;
        Ok(module.into())
    }

    async fn get_normalized_move_struct(
        &self,
        package: ObjectID,
        module_name: String,
        struct_name: String,
    ) -> RpcResult<SuiMoveNormalizedStruct> {
        let module = get_move_module(self, package, module_name).await?;
        let structs = module.structs;
        let identifier = Identifier::new(struct_name.as_str()).map_err(|e| anyhow!("{e}"))?;
        Ok(match structs.get(&identifier) {
            Some(struct_) => Ok(struct_.clone().into()),
            None => Err(anyhow!(
                "No struct was found with struct name {}",
                struct_name
            )),
        }?)
    }

    async fn get_normalized_move_function(
        &self,
        package: ObjectID,
        module_name: String,
        function_name: String,
    ) -> RpcResult<SuiMoveNormalizedFunction> {
        let module = get_move_module(self, package, module_name).await?;
        let functions = module.exposed_functions;
        let identifier = Identifier::new(function_name.as_str()).map_err(|e| anyhow!("{e}"))?;
        Ok(match functions.get(&identifier) {
            Some(function) => Ok(function.clone().into()),
            None => Err(anyhow!(
                "No function was found with function name {}",
                function_name
            )),
        }?)
    }

    async fn get_move_function_arg_types(
        &self,
        package: ObjectID,
        module: String,
        function: String,
    ) -> RpcResult<Vec<MoveFunctionArgType>> {
        let object_read = self
            .state
            .get_object_read(&package)
            .await
            .map_err(|e| anyhow!("{e}"))?;

        let normalized = match object_read {
            ObjectRead::Exists(_obj_ref, object, _layout) => match object.data {
                Data::Package(p) => normalize_modules(p.serialized_module_map().values())
                    .map_err(|e| anyhow!("{e}")),
                _ => Err(anyhow!("Object is not a package with ID {}", package)),
            },
            _ => Err(anyhow!("Package object does not exist with ID {}", package)),
        }?;

        let identifier = Identifier::new(function.as_str()).map_err(|e| anyhow!("{e}"))?;
        let parameters = normalized.get(&module).and_then(|m| {
            m.exposed_functions
                .get(&identifier)
                .map(|f| f.parameters.clone())
        });

        Ok(match parameters {
            Some(parameters) => Ok(parameters
                .iter()
                .map(|p| match p {
                    Type::Struct {
                        address: _,
                        module: _,
                        name: _,
                        type_arguments: _,
                    } => MoveFunctionArgType::Object(ObjectValueKind::ByValue),
                    Type::Reference(_) => {
                        MoveFunctionArgType::Object(ObjectValueKind::ByImmutableReference)
                    }
                    Type::MutableReference(_) => {
                        MoveFunctionArgType::Object(ObjectValueKind::ByMutableReference)
                    }
                    _ => MoveFunctionArgType::Pure,
                })
                .collect::<Vec<MoveFunctionArgType>>()),
            None => Err(anyhow!("No parameters found for function {}", function)),
        }?)
    }

    async fn get_transactions_by_input_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_by_input_object(object).await?)
    }

    async fn get_transactions_by_mutated_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .state
            .get_transactions_by_mutated_object(object)
            .await?)
    }

    async fn get_transactions_by_move_function(
        &self,
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .state
            .get_transactions_by_move_function(package, module, function)
            .await?)
    }

    async fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_from_addr(addr).await?)
    }

    async fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_to_addr(addr).await?)
    }
}

impl SuiRpcModule for FullNodeApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::RpcFullNodeReadApiOpenRpc::module_doc()
    }
}

pub async fn get_move_module(
    fullnode_api: &FullNodeApi,
    package: ObjectID,
    module_name: String,
) -> RpcResult<NormalizedModule> {
    let normalized = get_move_modules_by_package(fullnode_api, package).await?;
    Ok(match normalized.get(&module_name) {
        Some(module) => Ok(module.clone()),
        None => Err(anyhow!("No module found with module name {}", module_name)),
    }?)
}

pub async fn get_move_modules_by_package(
    fullnode_api: &FullNodeApi,
    package: ObjectID,
) -> RpcResult<BTreeMap<String, NormalizedModule>> {
    let object_read = fullnode_api
        .state
        .get_object_read(&package)
        .await
        .map_err(|e| anyhow!("{e}"))?;

    Ok(match object_read {
        ObjectRead::Exists(_obj_ref, object, _layout) => match object.data {
            Data::Package(p) => {
                normalize_modules(p.serialized_module_map().values()).map_err(|e| anyhow!("{e}"))
            }
            _ => Err(anyhow!("Object is not a package with ID {}", package)),
        },
        _ => Err(anyhow!("Package object does not exist with ID {}", package)),
    }?)
}
