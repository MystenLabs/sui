// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

/// Operational configurations of a consensus authority.
///
/// All fields should tolerate inconsistencies among authorities, without affecting safety of the
/// protocol. Otherwise, they need to be part of Sui protocol config or epoch state on-chain.
///
/// NOTE: default values should make sense, so most operators should not need to specify any field.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Parameters {
    /// Time to wait for parent round leader before sealing a block.
    /// Default: 250ms
    #[serde(default = "Parameters::default_leader_timeout")]
    pub leader_timeout: Duration,

    /// Minimum delay between rounds, to avoid generating too many rounds when latency is low.
    /// This is especially necessary for tests running locally.
    /// This should be set low enough, for example ~50ms, to avoid reducing round rate in
    /// realistic and distributed configurations.
    /// Default: 50ms
    #[serde(default = "Parameters::default_min_round_delay")]
    pub min_round_delay: Duration,

    /// Maximum forward time drift (how far in future) allowed for received blocks.
    /// Default: 500ms
    #[serde(default = "Parameters::default_max_forward_time_drift")]
    pub max_forward_time_drift: Duration,

    /// The database path.
    /// Required.
    pub db_path: Option<PathBuf>,
}

impl Parameters {
    pub fn default_leader_timeout() -> Duration {
        Duration::from_millis(250)
    }

    pub fn default_min_round_delay() -> Duration {
        Duration::from_millis(50)
    }

    pub fn default_max_forward_time_drift() -> Duration {
        Duration::from_millis(500)
    }

    pub fn db_path_str_unsafe(&self) -> String {
        self.db_path
            .clone()
            .expect("DB path is not set")
            .as_path()
            .to_str()
            .unwrap()
            .to_string()
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            leader_timeout: Parameters::default_leader_timeout(),
            min_round_delay: Parameters::default_min_round_delay(),
            max_forward_time_drift: Parameters::default_max_forward_time_drift(),
            db_path: None,
        }
    }
}
