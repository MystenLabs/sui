// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::Lazy;
use std::collections::BTreeSet;

mod validator {
    include!(concat!(env!("OUT_DIR"), "/sui.validator.Validator.rs"));
}

mod validator_paths {
    include!(concat!(env!("OUT_DIR"), "/sui.validator.paths.rs"));
}

pub use validator::{
    validator_client::ValidatorClient,
    validator_server::{Validator, ValidatorServer},
};

pub static KNOWN_VALIDATOR_GRPC_PATHS: Lazy<BTreeSet<&'static str>> = Lazy::new(|| {
    validator_paths::KNOWN_VALIDATOR_GRPC_PATHS_LIST
        .iter()
        .copied()
        .collect()
});
