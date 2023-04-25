// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::MoveUtilsServer;
use crate::read_api::{get_move_module, get_move_modules_by_package};
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_binary_format::file_format_common::VERSION_MAX;
use move_binary_format::normalized::Type;
use move_core_types::identifier::Identifier;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{
    MoveFunctionArgType, ObjectValueKind, SuiMoveNormalizedFunction, SuiMoveNormalizedModule,
    SuiMoveNormalizedStruct,
};
use sui_open_rpc::Module;
use sui_types::base_types::ObjectID;
use sui_types::move_package::normalize_modules;
use sui_types::object::{Data, ObjectRead};

pub struct MoveUtils {
    state: Arc<AuthorityState>,
}

impl MoveUtils {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

impl SuiRpcModule for MoveUtils {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::MoveUtilsOpenRpc::module_doc()
    }
}

#[async_trait]
impl MoveUtilsServer for MoveUtils {
    async fn get_normalized_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> RpcResult<BTreeMap<String, SuiMoveNormalizedModule>> {
        let modules = get_move_modules_by_package(&self.state, package).await?;
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
        let module = get_move_module(&self.state, package, module_name).await?;
        Ok(module.into())
    }

    async fn get_normalized_move_struct(
        &self,
        package: ObjectID,
        module_name: String,
        struct_name: String,
    ) -> RpcResult<SuiMoveNormalizedStruct> {
        let module = get_move_module(&self.state, package, module_name).await?;
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
        let module = get_move_module(&self.state, package, module_name).await?;
        let functions = module.functions;
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
            .map_err(|e| anyhow!("{e}"))?;

        let normalized = match object_read {
            ObjectRead::Exists(_obj_ref, object, _layout) => match object.data {
                Data::Package(p) => {
                    // we are on the read path - it's OK to use VERSION_MAX of the supported Move
                    // binary format
                    normalize_modules(
                        p.serialized_module_map().values(),
                        /* max_binary_format_version */ VERSION_MAX,
                        /* no_extraneous_module_bytes */ false,
                    )
                    .map_err(|e| anyhow!("{e}"))
                }
                _ => Err(anyhow!("Object is not a package with ID {}", package)),
            },
            _ => Err(anyhow!("Package object does not exist with ID {}", package)),
        }?;

        let identifier = Identifier::new(function.as_str()).map_err(|e| anyhow!("{e}"))?;
        let parameters = normalized
            .get(&module)
            .and_then(|m| m.functions.get(&identifier).map(|f| f.parameters.clone()));

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
}
