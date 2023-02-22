use anyhow::Result;
use clap::Parser;
// use prometheus::Registry;
use rand::Rng;
use sui_indexer::new_rpc_client;
use sui_json_rpc_types::CheckpointId;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    info!("Running benchmark setup in remote mode ..");
    let test_config = TestConfig::parse();
    let rpc_client = new_rpc_client(test_config.rpc_client_url.clone()).await?;

    let latest_checkpoint = rpc_client
        .read_api()
        .get_latest_checkpoint_sequence_number()
        .await?;

    let num = rand::thread_rng().gen_range(0..100);

    let target_checkpoint = latest_checkpoint - num;
    println!("{0:?}", target_checkpoint);

    let checkpoint = rpc_client
        .read_api()
        .get_checkpoint(CheckpointId::SequenceNumber(target_checkpoint))
        .await?;

    // let checkpoint = rpc_client
    //     .read_api()
    //     .get_checkpoint_summary(target_checkpoint)
    //     .await?;

    // let transactions = rpc_client.read_api()
    // TODO: grab indexer checkpoint

    // Assert transactions from both indexer and FN are the same length

    // compare FN checkpoint against IndexerCheckpoint
    let checkpoint_transactions = checkpoint.transactions;
    for i in 0..checkpoint_transactions.len() {
        println!("{0:?}", checkpoint_transactions.get(i));
    }

    Ok(())
}

#[derive(Parser)]
#[clap(name = "Transactions Test")]
pub struct TestConfig {
    #[clap(long)]
    pub rpc_client_url: String,
}
