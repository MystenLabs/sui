// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
pub use msim::*;

#[cfg(msim)]
use std::hash::Hasher;

use std::sync::atomic::{AtomicUsize, Ordering};

// Re-export things used by sui-macros
pub use ::rand as rand_crate;
pub use anemo;
pub use anemo_tower;
pub use fastcrypto;
pub use lru;
pub use move_package;
pub use mysten_network;
pub use sui_framework;
pub use sui_move_build;
pub use sui_types;
pub use telemetry_subscribers;
pub use tempfile;
pub use tower;

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

    /// A network with constant uniform latency.
    pub fn constant_latency_ms(latency: u64) -> SimConfig {
        uniform_latency_ms(latency..(latency + 1))
    }

    /// A network with latency sampled uniformly from a range.
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

    /// A network with bimodal latency.
    pub fn bimodal_latency_ms(
        // The typical latency.
        baseline: Range<u64>,
        // The exceptional latency.
        degraded: Range<u64>,
        // The frequency (from 0.0 to 1.0) with which the exceptional distribution is sampled.
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
        // Config to use when SUI_SIM_CONFIG is not set.
        default: SimConfig,
        // List of (&str, SimConfig) pairs - the SimConfig associated with the value
        // of the SUI_SIM_CONFIG var is chosen.
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

thread_local! {
    static NODE_COUNT: AtomicUsize = const { AtomicUsize::new(0) };
}

pub struct NodeLeakDetector(());

impl NodeLeakDetector {
    pub fn new() -> Self {
        NODE_COUNT.with(|c| c.fetch_add(1, Ordering::SeqCst));
        Self(())
    }

    pub fn get_current_node_count() -> usize {
        NODE_COUNT.with(|c| c.load(Ordering::SeqCst))
    }
}

impl Default for NodeLeakDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for NodeLeakDetector {
    fn drop(&mut self) {
        NODE_COUNT.with(|c| c.fetch_sub(1, Ordering::SeqCst));
    }
}

#[cfg(not(msim))]
#[macro_export]
macro_rules! return_if_killed {
    () => {};
}

#[cfg(msim)]
pub fn current_simnode_id() -> msim::task::NodeId {
    msim::runtime::NodeHandle::current().id()
}

#[cfg(msim)]
pub mod random {
    use super::*;

    use rand_crate::{rngs::SmallRng, thread_rng, Rng, SeedableRng};
    use serde::Serialize;
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::hash::Hash;

    /// Given a value, produce a random probability using the value as a seed, with
    /// an additional seed that is constant only for the current test thread.
    pub fn deterministic_probability<T: Hash>(value: T, chance: f32) -> bool {
        thread_local! {
            // a random seed that is shared by the whole test process, so that equal `value`
            // inputs produce different outputs when the test seed changes
            static SEED: u64 = thread_rng().gen();
        }

        chance
            > SEED.with(|seed| {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                seed.hash(&mut hasher);
                value.hash(&mut hasher);
                let mut rng = SmallRng::seed_from_u64(hasher.finish());
                rng.gen_range(0.0..1.0)
            })
    }

    /// Like deterministic_probability, but only returns true once for each unique value. May eventually
    /// consume all memory if there are a large number of unique, failing values.
    pub fn deterministic_probability_once<T: Hash + Serialize>(value: T, chance: f32) -> bool {
        thread_local! {
            static FAILING_VALUES: RefCell<HashSet<(msim::task::NodeId, Vec<u8>)>> = RefCell::new(HashSet::new());
        }

        let bytes = bcs::to_bytes(&value).unwrap();
        let key = (current_simnode_id(), bytes);

        FAILING_VALUES.with(|failing_values| {
            let mut failing_values = failing_values.borrow_mut();
            if failing_values.contains(&key) {
                false
            } else if deterministic_probability(value, chance) {
                failing_values.insert(key);
                true
            } else {
                false
            }
        })
    }
}
