use config::SharedCommittee;
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crypto::traits::VerifyingKey;
use multiaddr::Multiaddr;
use tonic::{Request, Response, Status};
use types::{Configuration, Empty, NewEpochRequest, NewNetworkInfoRequest, PublicKeyProto};

pub struct NarwhalConfiguration<PublicKey: VerifyingKey> {
    /// The committee
    committee: SharedCommittee<PublicKey>,
}

impl<PublicKey: VerifyingKey> NarwhalConfiguration<PublicKey> {
    pub fn new(committee: SharedCommittee<PublicKey>) -> Self {
        Self { committee }
    }

    /// Extracts and verifies the public key provided from the RoundsRequest.
    /// The method will return a result where the OK() will hold the
    /// parsed public key. The Err() will hold a Status message with the
    /// specific error description.
    fn get_public_key(&self, request: Option<&PublicKeyProto>) -> Result<PublicKey, Status> {
        let proto_key = request
            .ok_or_else(|| Status::invalid_argument("Invalid public key: no key provided"))?;
        let key = PublicKey::from_bytes(proto_key.bytes.as_ref())
            .map_err(|_| Status::invalid_argument("Invalid public key: couldn't parse"))?;

        // ensure provided key is part of the committee
        if self.committee.primary(&key).is_err() {
            return Err(Status::invalid_argument(
                "Invalid public key: unknown authority",
            ));
        }

        Ok(key)
    }
}

#[tonic::async_trait]
impl<PublicKey: VerifyingKey> Configuration for NarwhalConfiguration<PublicKey> {
    async fn new_epoch(
        &self,
        request: Request<NewEpochRequest>,
    ) -> Result<Response<Empty>, Status> {
        let new_epoch_request = request.into_inner();
        let epoch_number = new_epoch_request.epoch_number;
        let validators = new_epoch_request.validators;
        let mut parsed_input = vec![];
        for validator in validators.iter() {
            let public_key = self.get_public_key(validator.public_key.as_ref())?;

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

    async fn new_network_info(
        &self,
        request: Request<NewNetworkInfoRequest>,
    ) -> Result<Response<Empty>, Status> {
        let new_network_info_request = request.into_inner();
        let epoch_number = new_network_info_request.epoch_number;
        let validators = new_network_info_request.validators;
        let mut parsed_input = vec![];
        for validator in validators.iter() {
            let public_key = self.get_public_key(validator.public_key.as_ref())?;

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
