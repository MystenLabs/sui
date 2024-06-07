use todo::todo_client::TodoClient;
use todo::{CreateTodoRequest};

pub mod consensus {
    include!("consensus.rs");
}

struct GrpcClient{
    addr: SocketAddr,
}

impl GrpcClient {
    fn new(addr: String) -> Self {
        let addr = addr.parse().expect("Wrong address format!!!");
        Self { addr }
    }

    async fn connect(&self) -> Result<ConsensusApiClient<Channel>, Box<dyn std::error::Error>> {
        let mut client = ConsensusApiClient::connect(format!("http://{}", self.addr)).await?;
        Ok(client)
    }
    async fn sendTransaction(transaction: VerifiedTransaction) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stream = Box::pin(transaction);
        let response = client.init_transaction(stream).await?;
        let mut resp_stream = response.into_inner();

        while let Some(received) = resp_stream.next().await {
            match received {
                Ok(CommitedTransactions { transactions }) => {
                    info!("Received {:?} commited transactions.", transactions.len());
                    if let Err(err) = tx_commited_transactions.send(transactions) {
                        error!("{:?}", err);
                    }
                    //let _ = handler.handle_commited_transactions(transactions).await;
                }
                Err(err) => {
                    return Err(Box::new(err));
                }
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = GreeterClient::connect("http://[::1]:50051").await?;

    let request = tonic::Request::new(HelloRequest {
        name: "Tonic".into(),
    });

    let response = client.say_hello(request).await?;

    println!("RESPONSE={:?}", response);

    Ok(())
}