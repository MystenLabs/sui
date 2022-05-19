// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crypto::{ed25519::Ed25519PublicKey, traits::ToFromBytes};
use multiaddr::Multiaddr;
use tonic::{Request, Response, Status};
use types::{Configuration, Empty, NewNetworkInfoRequest};

#[derive(Debug)]
pub struct NarwhalConfiguration {}

impl NarwhalConfiguration {
    pub fn new() -> Self {
        Self {}
    }
}

#[tonic::async_trait]
impl Configuration for NarwhalConfiguration {
    async fn new_network_info(
        &self,
        request: Request<NewNetworkInfoRequest>,
    ) -> Result<Response<Empty>, Status> {
        let new_network_info_request = request.into_inner();
        let epoch_number = new_network_info_request.epoch_number;
        let validators = new_network_info_request.validators;
        let mut parsed_input = vec![];
        for validator in validators.iter() {
            let proto_key = validator
                .public_key
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Missing public key"))?;
            let public_key: Ed25519PublicKey =
                Ed25519PublicKey::from_bytes(proto_key.bytes.as_ref()).map_err(|err| {
                    Status::invalid_argument(format!("Could not serialize: {:?}", err))
                })?;

            let stake_weight = validator.stake_weight;
            let address: Multiaddr = validator
                .address
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Missing address"))?
                .address
                .parse()
                .map_err(|err| {
                    Status::invalid_argument(format!("Could not serialize: {:?}", err))
                })?;
            parsed_input.push(format!(
                "public_key: {:?} stake_weight: {:?} address: {:?}",
                public_key, stake_weight, address
            ));
        }
        Err(Status::internal(format!(
            "Not Implemented! But parsed input - epoch_number: {:?} & validator_data: {:?}",
            epoch_number, parsed_input
        )))
    }
}
