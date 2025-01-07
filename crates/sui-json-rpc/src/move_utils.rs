// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_state::StateRead;
use crate::error::{Error, SuiRpcInputError};
use crate::{with_tracing, SuiRpcModule};
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
#[cfg(test)]
use mockall::automock;
use move_binary_format::{
    binary_config::BinaryConfig,
    normalized::{Module as NormalizedModule, Type},
};
use move_core_types::identifier::Identifier;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_json_rpc_api::{MoveUtilsOpenRpc, MoveUtilsServer};
use sui_json_rpc_types::{
    MoveFunctionArgType, ObjectValueKind, SuiMoveNormalizedFunction, SuiMoveNormalizedModule,
    SuiMoveNormalizedStruct,
};
use sui_open_rpc::Module;
use sui_types::base_types::ObjectID;
use sui_types::move_package::normalize_modules;
use sui_types::object::{Data, ObjectRead};
use tap::TapFallible;
use tracing::{error, instrument, warn};

#[cfg_attr(test, automock)]
#[async_trait]
pub trait MoveUtilsInternalTrait {
    fn get_state(&self) -> &dyn StateRead;

    async fn get_move_module(
        &self,
        package: ObjectID,
        module_name: String,
    ) -> Result<NormalizedModule, Error>;

    async fn get_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> Result<BTreeMap<String, NormalizedModule>, Error>;

    fn get_object_read(&self, package: ObjectID) -> Result<ObjectRead, Error>;
}

pub struct MoveUtilsInternal {
    state: Arc<dyn StateRead>,
}

impl MoveUtilsInternal {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl MoveUtilsInternalTrait for MoveUtilsInternal {
    fn get_state(&self) -> &dyn StateRead {
        Arc::as_ref(&self.state)
    }

    async fn get_move_module(
        &self,
        package: ObjectID,
        module_name: String,
    ) -> Result<NormalizedModule, Error> {
        let normalized = self.get_move_modules_by_package(package).await?;
        Ok(match normalized.get(&module_name) {
            Some(module) => Ok(module.clone()),
            None => Err(SuiRpcInputError::GenericNotFound(format!(
                "No module found with module name {}",
                module_name
            ))),
        }?)
    }

    async fn get_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> Result<BTreeMap<String, NormalizedModule>, Error> {
        let object_read = self.get_state().get_object_read(&package).tap_err(|_| {
            warn!("Failed to call get_move_modules_by_package for package: {package:?}");
        })?;

        match object_read {
            ObjectRead::Exists(_obj_ref, object, _layout) => {
                match object.into_inner().data {
                    Data::Package(p) => {
                        // we are on the read path - it's OK to use VERSION_MAX of the supported Move
                        // binary format
                        let binary_config = BinaryConfig::with_extraneous_bytes_check(false);
                        normalize_modules(
                            p.serialized_module_map().values(),
                            &binary_config,
                        )
                        .map_err(|e| {
                            error!("Failed to call get_move_modules_by_package for package: {package:?}");
                            Error::from(e)
                        })
                    }
                    _ => Err(SuiRpcInputError::GenericInvalid(format!(
                        "Object is not a package with ID {}",
                        package
                    )))?,
                }
            }
            _ => Err(SuiRpcInputError::GenericNotFound(format!(
                "Package object does not exist with ID {}",
                package
            )))?,
        }
    }

    fn get_object_read(&self, package: ObjectID) -> Result<ObjectRead, Error> {
        self.state.get_object_read(&package).map_err(Error::from)
    }
}

pub struct MoveUtils {
    internal: Arc<dyn MoveUtilsInternalTrait + Send + Sync>,
}

impl MoveUtils {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self {
            internal: Arc::new(MoveUtilsInternal::new(state))
                as Arc<dyn MoveUtilsInternalTrait + Send + Sync>,
        }
    }
}

impl SuiRpcModule for MoveUtils {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        MoveUtilsOpenRpc::module_doc()
    }
}

#[async_trait]
impl MoveUtilsServer for MoveUtils {
    #[instrument(skip(self))]
    async fn get_normalized_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> RpcResult<BTreeMap<String, SuiMoveNormalizedModule>> {
        with_tracing!(async move {
            let modules = self.internal.get_move_modules_by_package(package).await?;
            Ok(modules
                .into_iter()
                .map(|(name, module)| (name, module.into()))
                .collect::<BTreeMap<String, SuiMoveNormalizedModule>>())
        })
    }

    #[instrument(skip(self))]
    async fn get_normalized_move_module(
        &self,
        package: ObjectID,
        module_name: String,
    ) -> RpcResult<SuiMoveNormalizedModule> {
        with_tracing!(async move {
            let module = self.internal.get_move_module(package, module_name).await?;
            Ok(module.into())
        })
    }

    #[instrument(skip(self))]
    async fn get_normalized_move_struct(
        &self,
        package: ObjectID,
        module_name: String,
        struct_name: String,
    ) -> RpcResult<SuiMoveNormalizedStruct> {
        with_tracing!(async move {
            let module = self.internal.get_move_module(package, module_name).await?;
            let structs = module.structs;
            let identifier = Identifier::new(struct_name.as_str())
                .map_err(|e| SuiRpcInputError::GenericInvalid(format!("{e}")))?;
            match structs.get(&identifier) {
                Some(struct_) => Ok(struct_.clone().into()),
                None => Err(SuiRpcInputError::GenericNotFound(format!(
                    "No struct was found with struct name {}",
                    struct_name
                )))?,
            }
        })
    }

    #[instrument(skip(self))]
    async fn get_normalized_move_function(
        &self,
        package: ObjectID,
        module_name: String,
        function_name: String,
    ) -> RpcResult<SuiMoveNormalizedFunction> {
        with_tracing!(async move {
            let module = self.internal.get_move_module(package, module_name).await?;
            let functions = module.functions;
            let identifier = Identifier::new(function_name.as_str())
                .map_err(|e| SuiRpcInputError::GenericInvalid(format!("{e}")))?;
            match functions.get(&identifier) {
                Some(function) => Ok(function.clone().into()),
                None => Err(SuiRpcInputError::GenericNotFound(format!(
                    "No function was found with function name {}",
                    function_name
                )))?,
            }
        })
    }

    #[instrument(skip(self))]
    async fn get_move_function_arg_types(
        &self,
        package: ObjectID,
        module: String,
        function: String,
    ) -> RpcResult<Vec<MoveFunctionArgType>> {
        with_tracing!(async move {
            let object_read = self.internal.get_object_read(package)?;

            let normalized = match object_read {
                ObjectRead::Exists(_obj_ref, object, _layout) => match object.into_inner().data {
                    Data::Package(p) => {
                        // we are on the read path - it's OK to use VERSION_MAX of the supported Move
                        // binary format
                        let binary_config = BinaryConfig::with_extraneous_bytes_check(false);
                        normalize_modules(p.serialized_module_map().values(), &binary_config)
                            .map_err(Error::from)
                    }
                    _ => Err(SuiRpcInputError::GenericInvalid(format!(
                        "Object is not a package with ID {}",
                        package
                    )))?,
                },
                _ => Err(SuiRpcInputError::GenericNotFound(format!(
                    "Package object does not exist with ID {}",
                    package
                )))?,
            }?;

            let identifier = Identifier::new(function.as_str())
                .map_err(|e| SuiRpcInputError::GenericInvalid(format!("{e}")))?;
            let parameters = normalized
                .get(&module)
                .and_then(|m| m.functions.get(&identifier).map(|f| f.parameters.clone()));

            match parameters {
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
                None => Err(SuiRpcInputError::GenericNotFound(format!(
                    "No parameters found for function {}",
                    function
                )))?,
            }
        })
    }
}

#[cfg(test)]
mod tests {

    mod get_normalized_move_module_tests {
        use super::super::*;
        use move_binary_format::file_format::basic_test_module;

        fn setup() -> (ObjectID, String) {
            (ObjectID::random(), String::from("test_module"))
        }

        #[tokio::test]
        async fn test_success_response() {
            let (package, module_name) = setup();
            let mut mock_internal = MockMoveUtilsInternalTrait::new();

            let m = basic_test_module();
            let normalized_module = NormalizedModule::new(&m);
            let expected_module: SuiMoveNormalizedModule = normalized_module.clone().into();

            mock_internal
                .expect_get_move_module()
                .return_once(move |_package, _module_name| Ok(normalized_module));

            let move_utils = MoveUtils {
                internal: Arc::new(mock_internal),
            };

            let response = move_utils
                .get_normalized_move_module(package, module_name)
                .await;

            assert!(response.is_ok());
            let result = response.unwrap();
            assert_eq!(result, expected_module);
        }

        #[tokio::test]
        async fn test_no_module_found() {
            let (package, module_name) = setup();
            let mut mock_internal = MockMoveUtilsInternalTrait::new();
            let error_string = format!("No module found with module name {module_name}");
            let expected_error =
                Error::SuiRpcInputError(SuiRpcInputError::GenericNotFound(error_string.clone()));
            mock_internal
                .expect_get_move_module()
                .return_once(move |_package, _module_name| Err(expected_error));
            let move_utils = MoveUtils {
                internal: Arc::new(mock_internal),
            };

            let response = move_utils
                .get_normalized_move_module(package, module_name)
                .await;
            let error_object = response.unwrap_err();

            assert_eq!(error_object.code(), -32602);
            assert_eq!(error_object.message(), &error_string);
        }
    }
}
