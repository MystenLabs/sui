use std::{
    fmt::{Debug, Display},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use super::measurement::MeasurementsCollection;

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

pub enum LoadType {
    Fixed(Vec<usize>),

    // TODO: Use tps regression as additional signal.
    #[allow(dead_code)]
    Search {
        starting_load: usize,
        latency_increase_tolerance: usize,
        max_iterations: usize,
    },
}

pub struct BenchmarkParametersGenerator {
    shared_objects_ration: u16,
    pub nodes: usize,
    load_type: LoadType,
    pub faults: usize,
    duration: Duration,
    next_load: Option<usize>,

    lower_bound_result: Option<MeasurementsCollection>,
    upper_bound_result: Option<MeasurementsCollection>,
    iterations: usize,
}

impl BenchmarkParametersGenerator {
    const DEFAULT_DURATION: Duration = Duration::from_secs(180);

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
            shared_objects_ration,
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

    pub fn with_faults(mut self, faults: usize) -> Self {
        self.faults = faults;
        self
    }

    pub fn with_custom_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

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

    pub fn next_parameters(&mut self) -> Option<BenchmarkParameters> {
        self.next_load.map(|load| {
            BenchmarkParameters::new(
                self.shared_objects_ration,
                self.nodes,
                self.faults,
                load,
                self.duration.clone(),
            )
        })
    }
}
