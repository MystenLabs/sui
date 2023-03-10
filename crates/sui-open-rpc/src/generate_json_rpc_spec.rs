// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::Write;

use clap::ArgEnum;
use clap::Parser;
use pretty_assertions::assert_str_eq;
use sui_core::SUI_CORE_VERSION;

use sui_json_rpc::coin_api::CoinReadApi;
use sui_json_rpc::event_api::EventReadApi;
use sui_json_rpc::governance_api::GovernanceReadApi;
use sui_json_rpc::read_api::ReadApi;
use sui_json_rpc::sui_rpc_doc;
use sui_json_rpc::transaction_builder_api::TransactionBuilderApi;
use sui_json_rpc::transaction_execution_api::TransactionExecutionApi;
use sui_json_rpc::SuiRpcModule;

use crate::examples::RpcExampleProvider;

mod examples;

#[derive(Debug, Parser, Clone, Copy, ArgEnum)]
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
    #[clap(arg_enum, default_value = "Record", ignore_case = true)]
    action: Action,
}

const FILE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/openrpc.json",);

#[tokio::main]
async fn main() {
    let options = Options::parse();

    let mut open_rpc = sui_rpc_doc(SUI_CORE_VERSION);
    open_rpc.add_module(ReadApi::rpc_doc_module());
    open_rpc.add_module(CoinReadApi::rpc_doc_module());
    open_rpc.add_module(EventReadApi::rpc_doc_module());
    open_rpc.add_module(TransactionExecutionApi::rpc_doc_module());
    open_rpc.add_module(TransactionBuilderApi::rpc_doc_module());
    open_rpc.add_module(GovernanceReadApi::rpc_doc_module());

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
