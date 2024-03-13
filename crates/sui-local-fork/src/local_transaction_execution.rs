// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::local_authority::LocalAuthority;
use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use fastcrypto::traits::ToFromBytes;
use jsonrpsee::core::RpcResult;
use jsonrpsee::core::__reexports::serde;
use std::net::SocketAddr;
use std::sync::Arc;
use sui_json_rpc::error::SuiRpcInputError;
use sui_json_rpc_api::WriteApiServer;
use sui_json_rpc_types::{
    DevInspectArgs, DevInspectResults, DryRunTransactionBlockResponse, SuiTransactionBlock,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_types::base_types::SuiAddress;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::signature::GenericSignature;
use sui_types::sui_serde::BigInt;
use sui_types::transaction::{InputObjectKind, Transaction, TransactionData, TransactionDataAPI};

pub struct LocalTransactionExecutionApi {
    state: Arc<LocalAuthority>,
}

impl LocalTransactionExecutionApi {
    pub fn new(state: Arc<LocalAuthority>) -> Self {
        LocalTransactionExecutionApi { state }
    }

    pub fn convert_bytes<T: serde::de::DeserializeOwned>(
        &self,
        tx_bytes: Base64,
    ) -> Result<T, SuiRpcInputError> {
        let data: T = bcs::from_bytes(&tx_bytes.to_vec()?)?;
        Ok(data)
    }

    #[allow(clippy::type_complexity)]
    fn prepare_execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        opts: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> Result<
        (
            SuiTransactionBlockResponseOptions,
            ExecuteTransactionRequestType,
            SuiAddress,
            Vec<InputObjectKind>,
            Transaction,
            Option<SuiTransactionBlock>,
            Vec<u8>,
        ),
        SuiRpcInputError,
    > {
        let opts = opts.unwrap_or_default();
        let request_type = match (request_type, opts.require_local_execution()) {
            (Some(ExecuteTransactionRequestType::WaitForEffectsCert), true) => {
                Err(SuiRpcInputError::InvalidExecuteTransactionRequestType)?
            }
            (t, _) => t.unwrap_or_else(|| opts.default_execution_request_type()),
        };
        let tx_data: TransactionData = self.convert_bytes(tx_bytes)?;
        let sender = tx_data.sender();
        let input_objs = tx_data.input_objects().unwrap_or_default();

        let mut sigs = Vec::new();
        for sig in signatures {
            sigs.push(GenericSignature::from_bytes(&sig.to_vec()?)?);
        }
        let txn = Transaction::from_generic_sig_data(tx_data, sigs);
        let raw_transaction = if opts.show_raw_input {
            bcs::to_bytes(txn.data())?
        } else {
            vec![]
        };
        let transaction = if opts.show_input {
            Some(SuiTransactionBlock::try_from(
                txn.data().clone(),
                &self.state.local_exec,
            )?)
        } else {
            None
        };
        Ok((
            opts,
            request_type,
            sender,
            input_objs,
            txn,
            transaction,
            raw_transaction,
        ))
    }
}
#[async_trait]
impl WriteApiServer for LocalTransactionExecutionApi {
    async fn execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        opts: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        let (opts, request_type, sender, input_objs, txn, transaction, raw_transaction) =
            self.prepare_execute_transaction_block(tx_bytes, signatures, opts, request_type)?;
        let digest = *txn.digest();

        // let fast_path_options = SuiTransactionBlockResponseOptions::full_content();
        // let sui_transaction_response = self
        //     .fullnode
        //     .execute_transaction_block(tx_bytes, signatures, Some(fast_path_options), request_type)
        //     .await?;
        //
        // Ok(SuiTransactionBlockResponseWithOptions {
        //     response: sui_transaction_response,
        //     options: options.unwrap_or_default(),
        // }
        //     .into())

        todo!();
    }

    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt<u64>>,
        epoch: Option<BigInt<u64>>,
        additional_args: Option<DevInspectArgs>,
    ) -> RpcResult<DevInspectResults> {
        // self.fullnode
        //     .dev_inspect_transaction_block(
        //         sender_address,
        //         tx_bytes,
        //         gas_price,
        //         epoch,
        //         additional_args,
        //     )
        //     .await
        todo!();
    }

    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse> {
        // self.fullnode.dry_run_transaction_block(tx_bytes).await
        todo!();
    }

    async fn monitored_execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        opts: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
        client_addr: Option<SocketAddr>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        todo!();
    }
}
