// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::SharedCommittee;
use consensus::dag::Dag;
use crypto::PublicKey;
use fastcrypto::traits::ToFromBytes;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use types::{
    NodeReadCausalRequest, NodeReadCausalResponse, Proposer, PublicKeyProto, RoundsRequest,
    RoundsResponse,
};

pub struct NarwhalProposer {
    /// The dag that holds the available certificates to propose
    dag: Option<Arc<Dag>>,

    /// The committee
    committee: SharedCommittee,
}

impl NarwhalProposer {
    pub fn new(dag: Option<Arc<Dag>>, committee: SharedCommittee) -> Self {
        Self { dag, committee }
    }

    /// Extracts and verifies the public key provided from the RoundsRequest.
    /// The method will return a result where the OK() will hold the
    /// parsed public key. The Err() will hold a Status message with the
    /// specific error description.
    fn get_public_key(&self, request: Option<PublicKeyProto>) -> Result<PublicKey, Status> {
        let proto_key = request
            .ok_or_else(|| Status::invalid_argument("Invalid public key: no key provided"))?;
        let key = PublicKey::from_bytes(proto_key.bytes.as_ref())
            .map_err(|_| Status::invalid_argument("Invalid public key: couldn't parse"))?;

        // ensure provided key is part of the committee
        if self.committee.load().primary(&key).is_err() {
            return Err(Status::invalid_argument(
                "Invalid public key: unknown authority",
            ));
        }

        Ok(key)
    }
}

#[tonic::async_trait]
impl Proposer for NarwhalProposer {
    /// Retrieves the min & max rounds that contain collections available for
    /// block proposal for the dictated validator.
    /// by the provided public key.
    async fn rounds(
        &self,
        request: Request<RoundsRequest>,
    ) -> Result<Response<RoundsResponse>, Status> {
        let key = self.get_public_key(request.into_inner().public_key)?;

        // call the dag to retrieve the rounds
        if let Some(dag) = &self.dag {
            let result = match dag.rounds(key).await {
                Ok(r) => Ok(RoundsResponse {
                    oldest_round: *r.start(),
                    newest_round: *r.end(),
                }),
                Err(err) => Err(Status::internal(format!("Couldn't retrieve rounds: {err}"))),
            };
            return result.map(Response::new);
        }

        Err(Status::internal("Can not serve request"))
    }

    async fn node_read_causal(
        &self,
        request: Request<NodeReadCausalRequest>,
    ) -> Result<Response<NodeReadCausalResponse>, Status> {
        let node_read_causal_request = request.into_inner();

        let key = self.get_public_key(node_read_causal_request.public_key)?;
        let round = node_read_causal_request.round;

        if let Some(dag) = &self.dag {
            let result = match dag.node_read_causal(key, round).await {
                Ok(digests) => Ok(NodeReadCausalResponse {
                    collection_ids: digests.into_iter().map(Into::into).collect(),
                }),
                Err(err) => Err(Status::internal(format!(
                    "Couldn't read causal for provided key & round: {err}"
                ))),
            };
            return result.map(Response::new);
        }
        Err(Status::internal("Dag does not exist"))
    }
}
