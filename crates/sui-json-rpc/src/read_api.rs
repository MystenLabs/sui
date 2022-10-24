// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee_core::server::rpc_module::RpcModule;
use move_binary_format::normalized::{Module as NormalizedModule, Type};
use move_core_types::identifier::Identifier;
use signature::Signature;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{
    GetObjectDataResponse, GetPastObjectDataResponse, MoveFunctionArgType, ObjectValueKind, Page,
    SuiMoveNormalizedFunction, SuiMoveNormalizedModule, SuiMoveNormalizedStruct, SuiObjectInfo,
    SuiTransactionEffects, SuiTransactionResponse, TransactionsPage,
};
use sui_open_rpc::Module;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::batch::TxSequenceNumber;
use sui_types::committee::EpochId;
use sui_types::crypto::SignatureScheme;
use sui_types::intent::IntentMessage;
use sui_types::messages::{
    CommitteeInfoRequest, CommitteeInfoResponse, Transaction, TransactionData,
};
use sui_types::move_package::normalize_modules;
use sui_types::object::{Data, ObjectRead, Owner};
use sui_types::query::{Ordering, TransactionQuery};
use sui_types::sui_serde::Base64;

use crate::api::RpcReadApiServer;
use crate::api::{RpcFullNodeReadApiServer, MAX_RESULT_SIZE};
use crate::SuiRpcModule;

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
        let (cert, effects) = self.state.get_transaction(digest).await?;
        Ok(SuiTransactionResponse {
            certificate: cert.try_into()?,
            effects: SuiTransactionEffects::try_from(effects, self.state.module_cache.as_ref())?,
            timestamp_ms: self.state.get_timestamp_ms(&digest).await?,
            parsed_data: None,
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
    async fn dry_run_transaction(
        &self,
        tx_bytes: Base64,
        sig_scheme: SignatureScheme,
        signature: Base64,
        pub_key: Base64,
    ) -> RpcResult<SuiTransactionEffects> {
        let intent_msg = IntentMessage::<TransactionData>::from_bytes(&tx_bytes.to_vec()?)?;

        let flag = vec![sig_scheme.flag()];
        let signature =
            Signature::from_bytes(&[&*flag, &*signature.to_vec()?, &pub_key.to_vec()?].concat())
                .map_err(|e| anyhow!(e))?;
        let txn = Transaction::new(intent_msg.value, intent_msg.intent, signature);
        let txn_digest = *txn.digest();

        Ok(self.state.dry_run_transaction(&txn, txn_digest).await?)
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
        order: Ordering,
    ) -> RpcResult<TransactionsPage> {
        let limit = limit.unwrap_or(MAX_RESULT_SIZE);

        if limit == 0 {
            Err(anyhow!("Page result limit must be larger then 0."))?;
        }
        let reverse = order == Ordering::Descending;

        // Retrieve 1 extra item for next cursor
        let mut data = self
            .state
            .get_transactions(query, cursor, Some(limit + 1), reverse)?;

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
