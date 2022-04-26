// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::Write;

use clap::ArgEnum;
use clap::Parser;
use pretty_assertions::assert_str_eq;

use sui::rpc_gateway::RpcGatewayOpenRpc;

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
    #[clap(arg_enum, default_value = "Print", ignore_case = true)]
    action: Action,
}

const FILE_PATH: &str = "sui/open_rpc/spec/openrpc.json";

fn main() {
    let options = Options::parse();
    let open_rpc = RpcGatewayOpenRpc::open_rpc();
    match options.action {
        Action::Print => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            println!("{content}");
        }
        Action::Record => {
            let content = serde_json::to_string_pretty(&open_rpc).unwrap();
            let mut f = File::create(FILE_PATH).unwrap();
            writeln!(f, "{}", content).unwrap();
        }
        Action::Test => {
            let reference = std::fs::read_to_string(FILE_PATH).unwrap();
            let content = serde_json::to_string_pretty(&open_rpc).unwrap() + "\n";
            assert_str_eq!(&reference, &content);
        }
    }
}

#[test]
fn test_json_rpc_spec() {
    // If this test breaks and you intended a json rpc schema change, you need to run to get the fresh schema:
    // # cargo -q run --example generate-json-rpc-spec -- record
    let status = std::process::Command::new("cargo")
        .current_dir("..")
        .args(&["run", "--example", "generate-json-rpc-spec", "--"])
        .arg("test")
        .status()
        .expect("failed to execute process");
    assert!(status.success());
}
