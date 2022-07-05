// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{PrimaryAddresses, SharedCommittee};
use crypto::traits::VerifyingKey;
use std::collections::BTreeMap;
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
        if self.committee.load().primary(&key).is_err() {
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
            let primary_addresses = validator
                .primary_addresses
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Missing primary addresses"))?;
            let primary_to_primary = primary_addresses
                .primary_to_primary
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Missing primary to primary address"))?
                .address
                .parse()
                .map_err(|err| {
                    Status::invalid_argument(format!("Could not serialize: {:?}", err))
                })?;
            let worker_to_primary = primary_addresses
                .primary_to_primary
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Missing worker to primary address"))?
                .address
                .parse()
                .map_err(|err| {
                    Status::invalid_argument(format!("Could not serialize: {:?}", err))
                })?;
            let primary = PrimaryAddresses {
                primary_to_primary,
                worker_to_primary,
            };
            parsed_input.push(format!(
                "public_key: {:?} stake_weight: {:?} primary addresses: {:?}",
                public_key, stake_weight, primary
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
        let epoch_number: u64 = new_network_info_request.epoch_number.into();
        if epoch_number != self.committee.load().epoch() {
            return Err(Status::invalid_argument(format!(
                "Passed in epoch {epoch_number} does not match current epoch {}",
                self.committee.load().epoch
            )));
        }
        let validators = new_network_info_request.validators;
        let mut new_network_info = BTreeMap::new();
        for validator in validators.iter() {
            let public_key = self.get_public_key(validator.public_key.as_ref())?;

            let stake_weight: u32 = validator
                .stake_weight
                .try_into()
                .map_err(|_| Status::invalid_argument("Invalid stake weight"))?;
            let primary_addresses = validator
                .primary_addresses
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Missing primary addresses"))?;
            let primary_to_primary = primary_addresses
                .primary_to_primary
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Missing primary to primary address"))?
                .address
                .parse()
                .map_err(|err| {
                    Status::invalid_argument(format!("Could not serialize: {:?}", err))
                })?;
            let worker_to_primary = primary_addresses
                .primary_to_primary
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Missing worker to primary address"))?
                .address
                .parse()
                .map_err(|err| {
                    Status::invalid_argument(format!("Could not serialize: {:?}", err))
                })?;
            let primary = PrimaryAddresses {
                primary_to_primary,
                worker_to_primary,
            };
            new_network_info.insert(public_key, (stake_weight, primary));
        }
        let mut new_committee = (**self.committee.load()).clone();
        let res = new_committee.update_primary_network_info(new_network_info);
        if res.is_ok() {
            self.committee.swap(std::sync::Arc::new(new_committee));
        }
        res.map_err(|err| Status::internal(format!("Could not update network info: {:?}", err)))?;

        Ok(Response::new(Empty {}))
    }
}
