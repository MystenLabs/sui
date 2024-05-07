// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{Debug, Display},
    hash::Hash,
    str::FromStr,
    time::Duration,
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{faults::FaultsType, measurement::MeasurementsCollection};

pub trait BenchmarkType:
    Serialize
    + DeserializeOwned
    + Default
    + Clone
    + FromStr
    + Display
    + Debug
    + PartialEq
    + Eq
    + Hash
    + PartialOrd
    + Ord
    + FromStr
{
}

/// The benchmark parameters for a run.
#[derive(Serialize, Deserialize, Clone)]
pub struct BenchmarkParameters<T> {
    /// The type of benchmark to run.
    pub benchmark_type: T,
    /// The committee size.
    pub nodes: usize,
    /// The number of (crash-)faults.
    pub faults: FaultsType,
    /// The total load (tx/s) to submit to the system.
    pub load: usize,
    /// The duration of the benchmark.
    pub duration: Duration,
}

impl<T: BenchmarkType> Default for BenchmarkParameters<T> {
    fn default() -> Self {
        Self {
            benchmark_type: T::default(),
            nodes: 4,
            faults: FaultsType::default(),
            load: 500,
            duration: Duration::from_secs(60),
        }
    }
}

impl<T: BenchmarkType> Debug for BenchmarkParameters<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}-{:?}-{}-{}",
            self.benchmark_type, self.faults, self.nodes, self.load
        )
    }
}

impl<T> Display for BenchmarkParameters<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} nodes ({}) - {} tx/s",
            self.nodes, self.faults, self.load
        )
    }
}

impl<T> BenchmarkParameters<T> {
    /// Make a new benchmark parameters.
    pub fn new(
        benchmark_type: T,
        nodes: usize,
        faults: FaultsType,
        load: usize,
        duration: Duration,
    ) -> Self {
        Self {
            benchmark_type,
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
pub struct BenchmarkParametersGenerator<T> {
    /// The type of benchmark to run.
    benchmark_type: T,
    /// The committee size.
    pub nodes: usize,
    /// The load type.
    load_type: LoadType,
    /// The number of faulty nodes.
    pub faults: FaultsType,
    /// The duration of the benchmark.
    duration: Duration,
    /// The load of the next benchmark run.
    next_load: Option<usize>,
    /// Temporary hold a lower bound of the breaking point.
    lower_bound_result: Option<MeasurementsCollection<T>>,
    /// Temporary hold an upper bound of the breaking point.
    upper_bound_result: Option<MeasurementsCollection<T>>,
    /// The current number of iterations.
    iterations: usize,
}

impl<T: BenchmarkType> Iterator for BenchmarkParametersGenerator<T> {
    type Item = BenchmarkParameters<T>;

    /// Return the next set of benchmark parameters to run.
    fn next(&mut self) -> Option<Self::Item> {
        self.next_load.map(|load| {
            BenchmarkParameters::new(
                self.benchmark_type.clone(),
                self.nodes,
                self.faults.clone(),
                load,
                self.duration,
            )
        })
    }
}

impl<T: BenchmarkType> BenchmarkParametersGenerator<T> {
    /// The default benchmark duration.
    const DEFAULT_DURATION: Duration = Duration::from_secs(180);

    /// make a new generator.
    pub fn new(nodes: usize, mut load_type: LoadType) -> Self {
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
            benchmark_type: T::default(),
            nodes,
            load_type,
            faults: FaultsType::default(),
            duration: Self::DEFAULT_DURATION,
            next_load,
            lower_bound_result: None,
            upper_bound_result: None,
            iterations: 0,
        }
    }

    /// Set the benchmark type.
    pub fn with_benchmark_type(mut self, benchmark_type: T) -> Self {
        self.benchmark_type = benchmark_type;
        self
    }

    /// Set crash-recovery pattern and the number of faulty nodes.
    pub fn with_faults(mut self, faults: FaultsType) -> Self {
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
        last_result: &MeasurementsCollection<T>,
        new_result: &MeasurementsCollection<T>,
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
    pub fn register_result(&mut self, result: MeasurementsCollection<T>) {
        self.next_load = match &mut self.load_type {
            LoadType::Fixed(loads) => {
                if loads.is_empty() {
                    None
                } else {
                    Some(loads.remove(0))
                }
            }
            LoadType::Search { max_iterations, .. } => {
                // Terminate the search.
                if self.iterations >= *max_iterations {
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
pub mod test {
    use std::{fmt::Display, str::FromStr};

    use serde::{Deserialize, Serialize};

    use crate::{
        measurement::{Measurement, MeasurementsCollection},
        settings::Settings,
    };

    use super::{BenchmarkParametersGenerator, BenchmarkType, LoadType};

    /// Mock benchmark type for unit tests.
    #[derive(
        Serialize, Deserialize, Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Default,
    )]
    pub struct TestBenchmarkType;

    impl Display for TestBenchmarkType {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestBenchmarkType")
        }
    }

    impl FromStr for TestBenchmarkType {
        type Err = ();

        fn from_str(_s: &str) -> Result<Self, Self::Err> {
            Ok(Self {})
        }
    }

    impl BenchmarkType for TestBenchmarkType {}

    #[test]
    fn set_lower_bound() {
        let settings = Settings::new_for_test();
        let nodes = 4;
        let load = LoadType::Search {
            starting_load: 100,
            max_iterations: 10,
        };
        let mut generator = BenchmarkParametersGenerator::<TestBenchmarkType>::new(nodes, load);
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
        let nodes = 4;
        let load = LoadType::Search {
            starting_load: 100,
            max_iterations: 10,
        };
        let mut generator = BenchmarkParametersGenerator::<TestBenchmarkType>::new(nodes, load);
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
        let nodes = 4;
        let load = LoadType::Search {
            starting_load: 100,
            max_iterations: 0,
        };
        let mut generator = BenchmarkParametersGenerator::<TestBenchmarkType>::new(nodes, load);
        let parameters = generator.next().unwrap();

        let collection = MeasurementsCollection::new(&settings, parameters);
        generator.register_result(collection);

        let next_parameters = generator.next();
        assert!(next_parameters.is_none());
    }
}
