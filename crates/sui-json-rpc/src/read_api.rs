// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use move_binary_format::normalized::{Module as NormalizedModule, Type};
use move_core_types::identifier::Identifier;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_types::intent::{Intent, IntentMessage};
use sui_types::sui_system_state::SuiSystemState;
use tap::TapFallible;

use fastcrypto::encoding::Base64;
use jsonrpsee::RpcModule;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{
    DynamicFieldPage, GetObjectDataResponse, GetPastObjectDataResponse, MoveFunctionArgType,
    ObjectValueKind, Page, SuiMoveNormalizedFunction, SuiMoveNormalizedModule,
    SuiMoveNormalizedStruct, SuiObjectInfo, SuiTransactionAuthSignersResponse,
    SuiTransactionEffects, SuiTransactionResponse, TransactionsPage,
};
use sui_open_rpc::Module;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::batch::TxSequenceNumber;
use sui_types::committee::EpochId;
use sui_types::crypto::sha3_hash;
use sui_types::messages::{CommitteeInfoRequest, CommitteeInfoResponse};
use sui_types::move_package::normalize_modules;
use sui_types::object::{Data, ObjectRead};
use sui_types::query::TransactionQuery;

use tracing::debug;

use crate::api::RpcFullNodeReadApiServer;
use crate::api::{cap_page_limit, RpcReadApiServer};
use crate::SuiRpcModule;

// An implementation of the read portion of the JSON-RPC interface intended for use in
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
            .get_owner_objects(address)
            .map_err(|e| anyhow!("{e}"))?
            .into_iter()
            .map(SuiObjectInfo::from)
            .collect())
    }

    // TODO: Remove this
    // This is very expensive, it's only for backward compatibilities and should be removed asap.
    async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> RpcResult<Vec<SuiObjectInfo>> {
        let dynamic_fields = self
            .state
            .get_dynamic_fields(object_id, None, usize::MAX)
            .map_err(|e| anyhow!("{e}"))?;

        let mut object_info = vec![];
        for info in dynamic_fields {
            let object = self
                .state
                .get_object_read(&info.object_id)
                .await
                .and_then(|read| read.into_object())
                .map_err(|e| anyhow!(e))?;
            object_info.push(SuiObjectInfo {
                object_id: object.id(),
                version: object.version(),
                digest: object.digest(),
                // Package cannot be owned by object, safe to unwrap.
                type_: format!("{}", object.type_().unwrap()),
                owner: object.owner,
                previous_transaction: object.previous_transaction,
            });
        }
        Ok(object_info)
    }

    async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage> {
        let limit = cap_page_limit(limit);
        let mut data = self
            .state
            .get_dynamic_fields(parent_object_id, cursor, limit + 1)
            .map_err(|e| anyhow!("{e}"))?;
        let next_cursor = data.get(limit).map(|info| info.object_id);
        data.truncate(limit);
        Ok(DynamicFieldPage { data, next_cursor })
    }

    async fn get_object(&self, object_id: ObjectID) -> RpcResult<GetObjectDataResponse> {
        Ok(self
            .state
            .get_object_read(&object_id)
            .await
            .map_err(|e| {
                debug!(?object_id, "Failed to get object: {:?}", e);
                anyhow!("{e}")
            })?
            .try_into()?)
    }

    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: String,
    ) -> RpcResult<GetObjectDataResponse> {
        let id = self
            .state
            .get_dynamic_field_object_id(parent_object_id, &name)
            .map_err(|e| anyhow!("{e}"))?
            .ok_or_else(|| {
                anyhow!("Cannot find dynamic field [{name}] for object [{parent_object_id}].")
            })?;
        self.get_object(id).await
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.state.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> RpcResult<Vec<TransactionDigest>> {
        Ok(self
            .state
            .get_transactions_in_range(start, end)?
            .into_iter()
            .map(|(_, digest)| digest)
            .collect())
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<SuiTransactionResponse> {
        let (cert, effects) = self
            .state
            .get_transaction(digest)
            .await
            .tap_err(|err| debug!(tx_digest=?digest, "Failed to get transaction: {:?}", err))?;
        Ok(SuiTransactionResponse {
            certificate: cert.try_into()?,
            effects: SuiTransactionEffects::try_from(effects, self.state.module_cache.as_ref())?,
            timestamp_ms: self.state.get_timestamp_ms(&digest).await?,
            parsed_data: None,
        })
    }

    async fn get_transaction_auth_signers(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<SuiTransactionAuthSignersResponse> {
        let (cert, _effects) = self
            .state
            .get_transaction(digest)
            .await
            .tap_err(|err| debug!(tx_digest=?digest, "Failed to get transaction: {:?}", err))?;

        let mut signers = Vec::new();
        let epoch_store = self.state.epoch_store();
        for authority_index in cert.auth_sig().signers_map.iter() {
            let authority = epoch_store
                .committee()
                .authority_by_index(authority_index)
                .ok_or_else(|| anyhow!("Failed to get authority"))?;
            signers.push(*authority);
        }

        Ok(SuiTransactionAuthSignersResponse { signers })
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
    async fn dry_run_transaction(&self, tx_bytes: Base64) -> RpcResult<SuiTransactionEffects> {
        let tx_data =
            bcs::from_bytes(&tx_bytes.to_vec().map_err(|e| anyhow!(e))?).map_err(|e| anyhow!(e))?;
        let intent_msg = IntentMessage::new(Intent::default(), tx_data);
        let txn_digest = TransactionDigest::new(sha3_hash(&intent_msg.value));
        Ok(self
            .state
            .dry_exec_transaction(intent_msg.value, txn_digest)
            .await?)
    }

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

    async fn get_transactions(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage> {
        let limit = cap_page_limit(limit);
        let descending = descending_order.unwrap_or_default();

        // Retrieve 1 extra item for next cursor
        let mut data = self
            .state
            .get_transactions(query, cursor, Some(limit + 1), descending)?;

        // extract next cursor
        let next_cursor = data.get(limit).cloned();
        data.truncate(limit);
        Ok(Page { data, next_cursor })
    }

    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> RpcResult<GetPastObjectDataResponse> {
        Ok(self
            .state
            .get_past_object_read(&object_id, version)
            .await
            .map_err(|e| anyhow!("{e}"))?
            .try_into()?)
    }

    async fn get_committee_info(&self, epoch: Option<EpochId>) -> RpcResult<CommitteeInfoResponse> {
        Ok(self
            .state
            .handle_committee_info_request(&CommitteeInfoRequest { epoch })
            .map_err(|e| anyhow!("{e}"))?)
    }

    async fn get_sui_system_state(&self) -> RpcResult<SuiSystemState> {
        Ok(self
            .state
            .get_sui_system_state_object()
            .await
            .map_err(|e| anyhow!("{e}"))?)
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
