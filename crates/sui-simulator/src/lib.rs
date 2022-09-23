// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
pub use msim::*;

// Re-export things used by sui-macros
pub use sui_framework;
pub use telemetry_subscribers;

#[cfg(msim)]
pub mod configs {
    use msim::*;
    use std::collections::HashMap;
    use std::ops::Range;
    use std::time::Duration;

    use tracing::info;

    fn ms_to_dur(range: Range<u64>) -> Range<Duration> {
        Duration::from_millis(range.start)..Duration::from_millis(range.end)
    }

    pub fn uniform_latency_ms(range: Range<u64>) -> SimConfig {
        let range = ms_to_dur(range);
        SimConfig {
            net: NetworkConfig {
                latency: LatencyConfig {
                    default_latency: LatencyDistribution::uniform(range),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    pub fn bimodal_latency_ms(
        baseline: Range<u64>,
        degraded: Range<u64>,
        degraded_freq: f64,
    ) -> SimConfig {
        let baseline = ms_to_dur(baseline);
        let degraded = ms_to_dur(degraded);
        SimConfig {
            net: NetworkConfig {
                latency: LatencyConfig {
                    default_latency: LatencyDistribution::bimodal(
                        baseline,
                        degraded,
                        degraded_freq,
                    ),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    /// Select from among a number of configs using the SUI_SIM_CONFIG env var.
    pub fn env_config(
        default: SimConfig,
        env_configs: impl IntoIterator<Item = (&'static str, SimConfig)>,
    ) -> SimConfig {
        let mut env_configs = HashMap::<&'static str, SimConfig>::from_iter(env_configs);
        if let Some(env) = std::env::var("SUI_SIM_CONFIG").ok() {
            if let Some(cfg) = env_configs.remove(env.as_str()) {
                info!("Using test config for SUI_SIM_CONFIG={}", env);
                cfg
            } else {
                panic!(
                    "No config found for SUI_SIM_CONFIG={}. Available configs are: {:?}",
                    env,
                    env_configs.keys()
                );
            }
        } else {
            info!("Using default test config");
            default
        }
    }
}
