// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod validator {
    include!(concat!(env!("OUT_DIR"), "/sui.validator.Validator.rs"));
}

pub use validator::{
    validator_client::ValidatorClient,
    validator_server::{Validator, ValidatorServer},
};
