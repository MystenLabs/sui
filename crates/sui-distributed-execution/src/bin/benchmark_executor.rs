use clap::*;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use sui_distributed_execution::ew_agent::*;
use sui_distributed_execution::server::*;
use sui_distributed_execution::sw_agent::*;
use sui_distributed_execution::types::*;
use sui_single_node_benchmark::command::*;
use sui_single_node_benchmark::run_benchmark;
use sui_single_node_benchmark::workload::Workload;

const FILE_PATH: &str = "crates/sui-distributed-execution/src/configs/1sw4ew.json";

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(long)]
    pub my_id: UniqueId,
    #[arg(
        long,
        default_value_t = 1_000,
        help = "Number of transactions to submit"
    )]
    pub tx_count: u64,

    #[arg(
        long, 
        default_value = FILE_PATH, 
        help = "Path to json config file"
    )]
    pub config_path: PathBuf,
}

#[tokio::main()]
async fn main() {
    // Parse command line
    let args = Args::parse();
    let my_id = args.my_id;
    let tx_count = args.tx_count;
    
    // Parse config from json
    let config_json = fs::read_to_string(args.config_path).expect("Failed to read config file");
    let mut global_config: HashMap<UniqueId, ServerConfig> =
    serde_json::from_str(&config_json).unwrap();
    assert!(
        global_config.contains_key(&my_id),
        "agent {} not in config",
        &my_id
    );
    global_config.
        entry(my_id).
        and_modify(|e| {
            e.attrs.insert("tx_count".to_string(), tx_count.to_string());
        }
    );

    // Initialize and run the server
    let kind = global_config.get(&my_id).unwrap().kind.as_str();
    if kind == "SW" {
        let mut server = Server::<SWAgent, SailfishMessage>::new(global_config, my_id);
        server.run().await;
    } else {
        // EW
        let mut server = Server::<EWAgent, SailfishMessage>::new(global_config, my_id);
        server.run().await;
    }
}
