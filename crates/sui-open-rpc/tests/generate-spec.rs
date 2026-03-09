// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::examples::RpcExampleProvider;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc::coin_api::CoinReadApi;
use sui_json_rpc::governance_api::GovernanceReadApi;
use sui_json_rpc::read_api::ReadApi;
use sui_json_rpc::sui_rpc_doc;
use sui_json_rpc::transaction_builder_api::TransactionBuilderApi;
use sui_json_rpc::transaction_execution_api::TransactionExecutionApi;
use sui_json_rpc_api::IndexerApiOpenRpc;
use sui_json_rpc_api::MoveUtilsOpenRpc;

mod examples;

// TODO: This currently always use workspace version, which is not ideal.
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[test]
#[cfg_attr(msim, ignore)]
fn test_json_rpc_spec() {
    let mut open_rpc = sui_rpc_doc(VERSION);
    open_rpc.add_module(ReadApi::rpc_doc_module());
    open_rpc.add_module(CoinReadApi::rpc_doc_module());
    open_rpc.add_module(IndexerApiOpenRpc::module_doc());
    open_rpc.add_module(TransactionExecutionApi::rpc_doc_module());
    open_rpc.add_module(TransactionBuilderApi::rpc_doc_module());
    open_rpc.add_module(GovernanceReadApi::rpc_doc_module());
    open_rpc.add_module(MoveUtilsOpenRpc::module_doc());
    open_rpc.add_examples(RpcExampleProvider::new().examples());

    let content = serde_json::to_string_pretty(&open_rpc).unwrap();
    insta::assert_binary_snapshot!("openrpc.json", content.into_bytes());
}
