// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
            "{} nodes ({} faulty) - {} tx/s",
            self.nodes, self.faults, self.load
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

impl Iterator for BenchmarkParametersGenerator {
    type Item = BenchmarkParameters;

    /// Return the next set of benchmark parameters to run.
    fn next(&mut self) -> Option<Self::Item> {
        self.next_load.map(|load| {
            BenchmarkParameters::new(
                self.shared_objects_ratio,
                self.nodes,
                self.faults,
                load,
                self.duration,
            )
        })
    }
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

    /// Detects whether the latest benchmark parameters run the system out of capacity.
    fn out_of_capacity(
        last_result: &MeasurementsCollection,
        new_result: &MeasurementsCollection,
    ) -> bool {
        // We consider the system is out of capacity if the latency increased by over 5x with
        // respect to the latest run.
        let threshold = last_result.aggregate_average_latency() * 5;
        let high_latency = new_result.aggregate_average_latency() > threshold;

        // Or if the throughput is less than 2/3 of the input rate.
        let last_load = new_result.transaction_load() as u64;
        let no_throughput_increase = new_result.aggregate_tps() < (2 * last_load / 3);

        high_latency || no_throughput_increase
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
                max_iterations,
                starting_load,
            } => {
                // Make one search with a very high load to test the system's robustness.
                if self.iterations == *max_iterations {
                    Some(*starting_load * 100)

                // Terminate the the search.
                } else if self.iterations > *max_iterations {
                    None

                // Search for the breaking point.
                } else {
                    self.iterations += 1;
                    match (&mut self.lower_bound_result, &mut self.upper_bound_result) {
                        (None, None) => {
                            let next = result.transaction_load() * 2;
                            self.lower_bound_result = Some(result);
                            Some(next)
                        }
                        (Some(lower), None) => {
                            if Self::out_of_capacity(lower, &result) {
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
                            if Self::out_of_capacity(lower, &result) {
                                *upper = result;
                            } else {
                                *lower = result;
                            }
                            Some((lower.transaction_load() + upper.transaction_load()) / 2)
                        }
                        _ => panic!("Benchmark parameters generator is in an incoherent state"),
                    }
                }
            }
        };
    }
}

#[cfg(test)]
mod test {
    use crate::{
        measurement::{Measurement, MeasurementsCollection},
        settings::Settings,
    };

    use super::{BenchmarkParametersGenerator, LoadType};

    #[test]
    fn set_lower_bound() {
        let settings = Settings::new_for_test();
        let shared_objects_ratio = 0;
        let nodes = 4;
        let load = LoadType::Search {
            starting_load: 100,
            max_iterations: 10,
        };
        let mut generator = BenchmarkParametersGenerator::new(shared_objects_ratio, nodes, load);
        let parameters = generator.next().unwrap();

        let collection = MeasurementsCollection::new(&settings, parameters);
        generator.register_result(collection);

        let next_parameters = generator.next();
        assert!(next_parameters.is_some());
        assert_eq!(next_parameters.unwrap().load, 200);

        assert!(generator.lower_bound_result.is_some());
        assert_eq!(
            generator.lower_bound_result.unwrap().transaction_load(),
            100
        );
        assert!(generator.upper_bound_result.is_none());
    }

    #[test]
    fn set_upper_bound() {
        let settings = Settings::new_for_test();
        let shared_objects_ratio = 0;
        let nodes = 4;
        let load = LoadType::Search {
            starting_load: 100,
            max_iterations: 10,
        };
        let mut generator = BenchmarkParametersGenerator::new(shared_objects_ratio, nodes, load);
        let first_parameters = generator.next().unwrap();

        // Register a first result (zero latency). This sets the lower bound.
        let collection = MeasurementsCollection::new(&settings, first_parameters);
        generator.register_result(collection);
        let second_parameters = generator.next().unwrap();

        // Register a second result (with positive latency). This sets the upper bound.
        let mut collection = MeasurementsCollection::new(&settings, second_parameters);
        let measurement = Measurement::new_for_test();
        collection.scrapers.insert(1, vec![measurement]);
        generator.register_result(collection);

        // Ensure the next load is between the upper and the lower bound.
        let third_parameters = generator.next();
        assert!(third_parameters.is_some());
        assert_eq!(third_parameters.unwrap().load, 150);

        assert!(generator.lower_bound_result.is_some());
        assert_eq!(
            generator.lower_bound_result.unwrap().transaction_load(),
            100
        );
        assert!(generator.upper_bound_result.is_some());
        assert_eq!(
            generator.upper_bound_result.unwrap().transaction_load(),
            200
        );
    }

    #[test]
    fn max_iterations() {
        let settings = Settings::new_for_test();
        let shared_objects_ratio = 0;
        let nodes = 4;
        let load = LoadType::Search {
            starting_load: 100,
            max_iterations: 0,
        };
        let mut generator = BenchmarkParametersGenerator::new(shared_objects_ratio, nodes, load);
        let parameters = generator.next().unwrap();

        let collection = MeasurementsCollection::new(&settings, parameters);
        generator.register_result(collection);

        let next_parameters = generator.next();
        assert!(next_parameters.is_none());
    }
}
