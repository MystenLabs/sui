use std::collections::HashMap;
use std::fs;
use clap::*;
use sui_distributed_execution::network_agents::*;
use sui_distributed_execution::server::*;
use sui_distributed_execution::types::*;

const FILE_PATH:&str = "/Users/tonyzhang/Documents/UMich2023su/sui.nosync/crates/sui-distributed-execution/src/configs/simple_config.json";

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]

struct Args {
    #[clap(long)]
    pub my_id: UniqueId
}

#[tokio::main()]
async fn main() {
    // Parse config from json
    let config_json = fs::read_to_string(FILE_PATH)
        .expect("Failed to read config file");
    let global_config: HashMap<UniqueId, AppConfig> 
        = serde_json::from_str(&config_json).unwrap();   

    // Parse command line
    let args = Args::parse();
    let my_id = args.my_id;
    assert!(global_config.contains_key(&my_id), "agent {} not in config", &my_id);

    // Initialize and run the server
    let mut server = Server::new(global_config.clone(), my_id);
    let kind = global_config.get(&my_id).unwrap().kind.as_str();
    match kind {
        "echo" => server.run::<EchoAgent>().await,
        "ping" => server.run::<PingAgent>().await,
        _ => panic!("Invalid agent kind {}", kind),
    }
}
