use clap::Parser;

use sui_network::topology_crawler::explorer::Explorer;
use sui_network::topology_crawler::peer_source::AnemoPeerSource;
use sui_network::topology_crawler::seed_peers::{seed_peers, Network};

#[derive(Parser, Debug)]
#[command(name = "topology-crawler")]
struct Args {
    #[arg(long)]
    network: String,
}

#[tokio::main]
async fn main() {
    let _args = Args::parse();
    let _source = AnemoPeerSource::new();
    let _explorer = Explorer::new(vec![]);
    let _seeds = seed_peers(Network::Mainnet);
    todo!("run crawler and output JSON to stdout")
}
