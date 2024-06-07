use std::net::SocketAddr;
use consensus::consensus_api_client::ConsensusApiClient;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use tokio_stream::{StreamExt, Stream};
use tonic::{transport::Channel, Request, Response, Status, Streaming};
use consensus::{Transaction, CommitedTransaction, CommitedTransactions};
use std::thread::{self, sleep};
use std::time::Duration;


struct ConsensusTask {
    addr: SocketAddr,
}

impl ConsensusTask {
    fn new(addr: String) -> Self {
        let addr = addr.parse().expect("Wrong address format!!!");
        Self { addr }
    }

    async fn connect(&self) -> Result<ConsensusApiClient<Channel>, Box<dyn std::error::Error>> {
        let mut client = ConsensusApiClient::connect(format!("http://{}", self.addr)).await?;
        Ok(client)
    }

    async fn call_stream(client: &mut ConsensusApiClient<Channel>) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        
        let request_stream = async_stream::stream! {
            while let Some(tx) = rx.recv().await {
                info!("New rx from tx: {}", &tx);
                yield ConsensusRequest{tx: tx};
            }
        };

        let request_stream = Request::new(request_stream);

        let response_stream = client.start_stream(request_stream).await?;

        tokio::spawn(async move {
            for i in 0..1000 {
                let tx_clone = tx.clone();
                // let mut input = String::new();
                // print!("Enter tx: ");
                // std::io::stdin().read_line(&mut input);
                // let input = input.trim().to_string();
                let input = String::from(format!("Hello {}th", i));
                info!("New tx: {}", &input);
                tokio::spawn(async move {
                    thread::sleep(Duration::from_millis(100));
                    tx_clone.send(input).await.expect("Cannot send tx");
                });
            }
        });

        let mut inbound = response_stream.into_inner();

        // tokio::spawn(async move {
            
            while let Some(res) = inbound.next().await {
                let res = res.expect("Empty response");
                info!("Receive tx_hash: {}", res.tx_hash);
            }
        // });
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    // Set the created subscriber as the default for the application
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let my_consensus_task = ConsensusTask::new("127.0.0.1:3000".to_string());
    loop {
        match my_consensus_task.connect().await {
            Ok(mut client) => {
                info!("Succesfully connect server");
                ConsensusTask::call_stream(&mut client).await?;
                break;
            }
            _ => {
                info!("Cannot connect server, try again");
            }
        }
    }
    
    Ok(())
}