// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Config, ValidatorInfo};
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;
use sui_types::committee::EpochId;

#[derive(Serialize, Deserialize)]
pub struct GatewayConfig {
    pub epoch: EpochId,
    pub validator_set: Vec<ValidatorInfo>,
    pub send_timeout: Duration,
    pub recv_timeout: Duration,
    pub buffer_size: usize,
    pub db_folder_path: PathBuf,
}

impl Config for GatewayConfig {}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            epoch: 0,
            validator_set: vec![],
            send_timeout: Duration::from_micros(4000000),
            recv_timeout: Duration::from_micros(4000000),
            buffer_size: 650000,
            db_folder_path: Default::default(),
        }
    }
}
