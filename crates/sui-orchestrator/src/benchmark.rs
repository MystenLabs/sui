use std::{
    fmt::{Debug, Display},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::measurement::MeasurementsCollection;

/// The benchmark parameters for a run.
#[derive(Serialize, Deserialize, Clone)]
pub struct BenchmarkParameters {
    /// Percentage of shared vs owned objects; 0 means only owned objects and 100 means
    /// only shared objects.
    pub shared_objects_ratio: u16,
    /// The committee size.
    pub nodes: usize,
    /// The number of (crash-)faults.
    pub faults: usize,
    /// The total load (tx/s) to submit to the system.
    pub load: usize,
    /// The duration of the benchmark.
    pub duration: Duration,
}

impl Default for BenchmarkParameters {
    fn default() -> Self {
        Self {
            shared_objects_ratio: 0,
            nodes: 4,
            faults: 0,
            load: 500,
            duration: Duration::from_secs(60),
        }
    }
}

impl Debug for BenchmarkParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}",
            self.shared_objects_ratio, self.faults, self.nodes, self.load
        )
    }
}

impl Display for BenchmarkParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} nodes ({} faulty) - {} tx/s, {}% shared objects",
            self.nodes, self.faults, self.load, self.shared_objects_ratio
        )
    }
}

impl BenchmarkParameters {
    /// Make a new benchmark parameters.
    pub fn new(
        shared_objects_ratio: u16,
        nodes: usize,
        faults: usize,
        load: usize,
        duration: Duration,
    ) -> Self {
        Self {
            shared_objects_ratio,
            nodes,
            faults,
            load,
            duration,
        }
    }
}

/// The load type to submit to the nodes.
pub enum LoadType {
    /// Submit a fixed set of loads (one per benchmark run).
    Fixed(Vec<usize>),

    /// Search for the breaking point of the L-graph.
    // TODO: Doesn't work very well, use tps regression as additional signal.
    #[allow(dead_code)]
    Search {
        /// The initial load to test (and use a baseline).
        starting_load: usize,
        /// The maximum latency increase before deducing that the system is out of capacity.
        latency_increase_tolerance: usize,
        /// The maximum number of iterations before converging on a breaking point.
        max_iterations: usize,
    },
}

/// Generate benchmark parameters (one set of parameters per run).
// TODO: The rusty thing to do would be to implement Iter.
pub struct BenchmarkParametersGenerator {
    /// The ratio of shared and owned objects (as a percentage)/
    shared_objects_ratio: u16,
    /// The committee size.
    pub nodes: usize,
    /// The load type.
    load_type: LoadType,
    /// The number of faulty nodes.
    pub faults: usize,
    /// The duration of the benchmark.
    duration: Duration,
    /// The load of the next benchmark run.
    next_load: Option<usize>,
    /// Temporary hold a lower bound of the breaking point.
    lower_bound_result: Option<MeasurementsCollection>,
    /// Temporary hold an upper bound of the breaking point.
    upper_bound_result: Option<MeasurementsCollection>,
    /// The current number of iterations.
    iterations: usize,
}

impl BenchmarkParametersGenerator {
    /// The default benchmark duration.
    const DEFAULT_DURATION: Duration = Duration::from_secs(180);

    /// make a new generator.
    pub fn new(shared_objects_ration: u16, nodes: usize, mut load_type: LoadType) -> Self {
        let next_load = match &mut load_type {
            LoadType::Fixed(loads) => {
                if loads.is_empty() {
                    None
                } else {
                    Some(loads.remove(0))
                }
            }
            LoadType::Search { starting_load, .. } => Some(*starting_load),
        };
        Self {
            shared_objects_ratio: shared_objects_ration,
            nodes,
            load_type,
            faults: 0,
            duration: Self::DEFAULT_DURATION,
            next_load,
            lower_bound_result: None,
            upper_bound_result: None,
            iterations: 0,
        }
    }

    /// Set the number of faulty nodes.
    pub fn with_faults(mut self, faults: usize) -> Self {
        self.faults = faults;
        self
    }

    /// Set a custom benchmark duration.
    pub fn with_custom_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Register a new benchmark measurements collection. These results are used to determine
    /// whether the system reached its breaking point.
    pub fn register_result(&mut self, result: MeasurementsCollection) {
        self.next_load = match &mut self.load_type {
            LoadType::Fixed(loads) => {
                if loads.is_empty() {
                    None
                } else {
                    Some(loads.remove(0))
                }
            }
            LoadType::Search {
                latency_increase_tolerance,
                max_iterations,
                ..
            } => {
                if self.iterations >= *max_iterations {
                    None
                } else {
                    self.iterations += 1;
                    match (&mut self.lower_bound_result, &mut self.upper_bound_result) {
                        (None, None) => {
                            let next = result.transaction_load() * 2;
                            self.lower_bound_result = Some(result);
                            Some(next)
                        }
                        (Some(lower), None) => {
                            let threshold = lower.aggregate_average_latency()
                                * (*latency_increase_tolerance as u32);
                            if result.aggregate_average_latency() > threshold {
                                let next =
                                    (lower.transaction_load() + result.transaction_load()) / 2;
                                self.upper_bound_result = Some(result);
                                Some(next)
                            } else {
                                let next = result.transaction_load() * 2;
                                *lower = result;
                                Some(next)
                            }
                        }
                        (Some(lower), Some(upper)) => {
                            let threshold = lower.aggregate_average_latency()
                                * (*latency_increase_tolerance as u32);
                            if result.aggregate_average_latency() > threshold {
                                *upper = result;
                            } else {
                                *lower = result;
                            }
                            Some((lower.transaction_load() + upper.transaction_load()) / 2)
                        }
                        _ => panic!("Benchmark parameters builder is in an incoherent state"),
                    }
                }
            }
        };
    }

    /// Return the next set of benchmark parameters to run.
    pub fn next_parameters(&mut self) -> Option<BenchmarkParameters> {
        self.next_load.map(|load| {
            BenchmarkParameters::new(
                self.shared_objects_ratio,
                self.nodes,
                self.faults,
                load,
                self.duration.clone(),
            )
        })
    }
}
