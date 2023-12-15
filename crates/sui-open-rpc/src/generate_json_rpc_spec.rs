// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::examples::RpcExampleProvider;
use clap::Parser;
use clap::ValueEnum;
use pretty_assertions::assert_str_eq;
use std::fs::File;
use std::io::Write;
//temporarily remove api ref content for indexer methods
//use sui_json_rpc::api::ExtendedApiOpenRpc;
use sui_json_rpc::coin_api::CoinReadApi;
use sui_json_rpc::governance_api::GovernanceReadApi;
use sui_json_rpc::read_api::ReadApi;
use sui_json_rpc::sui_rpc_doc;
use sui_json_rpc::transaction_builder_api::TransactionBuilderApi;
use sui_json_rpc::transaction_execution_api::TransactionExecutionApi;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_api::IndexerApiOpenRpc;
use sui_json_rpc_api::MoveUtilsOpenRpc;

mod examples;

#[derive(Debug, Parser, Clone, Copy, ValueEnum)]
enum Action {
    Print,
    Test,
    Record,
}

#[derive(Debug, Parser)]
#[clap(
    name = "Sui format generator",
    about = "Trace serde (de)serialization to generate format descriptions for Sui types"
)]
struct Options {
    #[clap(value_enum, default_value = "Record", ignore_case = true)]
    action: Action,
}

const FILE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/openrpc.json",);

// TODO: This currently always use workspace version, which is not ideal.
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() {
    let options = Options::parse();

    let mut open_rpc = sui_rpc_doc(VERSION);
    open_rpc.add_module(ReadApi::rpc_doc_module());
    open_rpc.add_module(CoinReadApi::rpc_doc_module());
    open_rpc.add_module(IndexerApiOpenRpc::module_doc());
    open_rpc.add_module(TransactionExecutionApi::rpc_doc_module());
    open_rpc.add_module(TransactionBuilderApi::rpc_doc_module());
    open_rpc.add_module(GovernanceReadApi::rpc_doc_module());
    //temporarily remove api ref content for indexer methods
    //open_rpc.add_module(ExtendedApiOpenRpc::module_doc());
    open_rpc.add_module(MoveUtilsOpenRpc::module_doc());

    open_rpc.add_examples(RpcExampleProvider::new().examples());

    match options.action {
        Action::Print => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            println!("{content}");
        }
        Action::Record => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            let mut f = File::create(FILE_PATH).unwrap();
            writeln!(f, "{content}").unwrap();
        }
        Action::Test => {
            let reference = std::fs::read_to_string(FILE_PATH).unwrap();
            let content = serde_json::to_string_pretty(&open_rpc).unwrap() + "\n";
            assert_str_eq!(&reference, &content);
        }
    }
}
