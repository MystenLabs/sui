// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::SharedCommittee;
use crypto::PublicKey;
use fastcrypto::traits::ToFromBytes;
use multiaddr::Multiaddr;
use tonic::{Request, Response, Status};
use types::{
    Configuration, Empty, GetPrimaryAddressResponse, MultiAddrProto, NewEpochRequest,
    NewNetworkInfoRequest, PublicKeyProto,
};

pub struct NarwhalConfiguration {
    primary_address: Multiaddr,
    /// The committee
    committee: SharedCommittee,
}

impl NarwhalConfiguration {
    pub fn new(primary_address: Multiaddr, committee: SharedCommittee) -> Self {
        Self {
            primary_address,
            committee,
        }
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
impl Configuration for NarwhalConfiguration {
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
            let primary_address: Multiaddr = validator
                .primary_address
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Missing primary address"))?
                .address
                .parse()
                .map_err(|err| {
                    Status::invalid_argument(format!("Could not serialize: {:?}", err))
                })?;
            parsed_input.push(format!(
                "public_key: {:?} stake_weight: {:?} primary address: {:?}",
                public_key, stake_weight, primary_address
            ));
        }
        Err(Status::internal(format!(
            "Not Implemented! But parsed input - epoch_number: {:?} & validator_data: {:?}",
            epoch_number, parsed_input
        )))
    }

    #[allow(clippy::mutable_key_type)]
    async fn new_network_info(
        &self,
        _request: Request<NewNetworkInfoRequest>,
    ) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }

    async fn get_primary_address(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetPrimaryAddressResponse>, Status> {
        Ok(Response::new(GetPrimaryAddressResponse {
            primary_address: Some(MultiAddrProto {
                address: self.primary_address.to_string(),
            }),
        }))
    }
}
