use tonic::{transport::Server, Request, Response, Status};
use consensus::consensus_api_server::{ConsensusApiServer, ConsensusApi};
use consensus::{VerifiedTransaction, CommitedTransactions};

pub mod consensus {
    include!("consensus.rs");
}

#[derive(Debug, Default)]
pub struct MyConsensusApi {}

#[tonic::async_trait]
impl ConsensusApi for MyConsensusApi {
    async fn sendTransaction(
        &self,
        request: Request<VerifiedTransaction>,
    ) -> Result<Response<CommitedTransactions>, Status> {
        println!("Got a request: {:?}", request);

        let reply = HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };

        Ok(Response::new(reply))
    }
}