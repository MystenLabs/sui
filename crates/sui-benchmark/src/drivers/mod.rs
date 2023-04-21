// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use duration_str::parse;
use std::{str::FromStr, time::Duration};

pub mod bench_driver;
pub mod driver;
use comfy_table::{Cell, Color, ContentArrangement, Row, Table};
use hdrhistogram::{serialization::Serializer, Histogram};

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum Interval {
    Count(u64),
    Time(tokio::time::Duration),
}

impl Interval {
    pub fn is_unbounded(&self) -> bool {
        matches!(self, Interval::Time(tokio::time::Duration::MAX))
    }
}

impl FromStr for Interval {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(i) = s.parse() {
            Ok(Interval::Count(i))
        } else if let Ok(d) = parse(s) {
            Ok(Interval::Time(d))
        } else if "unbounded" == s {
            Ok(Interval::Time(tokio::time::Duration::MAX))
        } else {
            Err("Required integer number of cycles or time duration".to_string())
        }
    }
}

// wrapper which implements serde
#[allow(dead_code)]
#[derive(Debug)]
pub struct HistogramWrapper {
    histogram: Histogram<u64>,
}

impl serde::Serialize for HistogramWrapper {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut vec = Vec::new();
        hdrhistogram::serialization::V2Serializer::new()
            .serialize(&self.histogram, &mut vec)
            .map_err(|e| serde::ser::Error::custom(e.to_string()))?;
        serializer.serialize_bytes(&vec)
    }
}

impl<'de> serde::Deserialize<'de> for HistogramWrapper {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let vec: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        let histogram: Histogram<u64> = hdrhistogram::serialization::Deserializer::new()
            .deserialize(&mut &vec[..])
            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
        Ok(HistogramWrapper { histogram })
    }
}

// Stores the final stress statisicts of the test run.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct StressStats {
    pub cpu_usage: HistogramWrapper,
}

impl StressStats {
    pub fn update(&mut self, sample_stat: &StressStats) {
        self.cpu_usage
            .histogram
            .add(&sample_stat.cpu_usage.histogram)
            .unwrap();
    }

    pub fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(200)
            .set_header(vec!["metric", "p50", "p99"]);

        let mut row = Row::new();
        row.add_cell(Cell::new("cpu usage"));
        row.add_cell(Cell::new(self.cpu_usage.histogram.value_at_quantile(0.5)));
        row.add_cell(Cell::new(self.cpu_usage.histogram.value_at_quantile(0.99)));
        table.add_row(row);
        table
    }
}

/// Stores the final statistics of the test run.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct BenchmarkStats {
    pub duration: Duration,
    /// Number of transactions that ended in an error
    pub num_error_txes: u64,
    /// Number of transactions that were executed successfully
    pub num_success_txes: u64,
    /// Total number of commands in transactions that executed successfully
    pub num_success_cmds: u64,
    /// Total gas used
    pub total_gas_used: u64,
    pub latency_ms: HistogramWrapper,
}

impl BenchmarkStats {
    pub fn update(&mut self, duration: Duration, sample_stat: &BenchmarkStats) {
        self.duration = duration;
        self.num_error_txes += sample_stat.num_error_txes;
        self.num_success_txes += sample_stat.num_success_txes;
        self.num_success_cmds += sample_stat.num_success_cmds;
        self.total_gas_used += sample_stat.total_gas_used;
        self.latency_ms
            .histogram
            .add(&sample_stat.latency_ms.histogram)
            .unwrap();
    }
    pub fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_width(200)
            .set_header(vec![
                "duration(s)",
                "tps",
                "cps",
                "error%",
                "latency (min)",
                "latency (p50)",
                "latency (p99)",
                "gas used (MIST total)",
                "gas used/hr (MIST approx.)",
            ]);
        let mut row = Row::new();
        let duration = self.duration.max(Duration::from_secs(1));
        row.add_cell(Cell::new(duration.as_secs()));
        row.add_cell(Cell::new(self.num_success_txes / duration.as_secs()));
        row.add_cell(Cell::new(self.num_success_cmds / duration.as_secs()));
        row.add_cell(Cell::new(
            (100 * self.num_error_txes) as f32
                / (self.num_error_txes + self.num_success_txes) as f32,
        ));
        row.add_cell(Cell::new(self.latency_ms.histogram.min()));
        row.add_cell(Cell::new(self.latency_ms.histogram.value_at_quantile(0.5)));
        row.add_cell(Cell::new(self.latency_ms.histogram.value_at_quantile(0.99)));
        row.add_cell(Cell::new(format_num_with_separators(
            self.total_gas_used,
            3,
            ",",
        )));
        row.add_cell(Cell::new(format_num_with_separators(
            self.total_gas_used * 60 * 60 / duration.as_secs(),
            3,
            ",",
        )));
        table.add_row(row);
        table
    }
}

/// A comparison between an old and a new benchmark.
/// All differences are reported in terms of measuring improvements
/// (negative) or regressions (positive). That is, if an old benchmark
/// is slower than a new benchmark, then the difference is negative.
/// Conversely, if an old benchmark is faster than a new benchmark,
/// then the difference is positive.
#[derive(Clone, Debug)]
pub struct Comparison {
    pub name: String,
    pub old_value: String,
    pub new_value: String,
    pub diff: i64,
    pub diff_ratio: f64,
    pub speedup: f64,
}

pub struct BenchmarkCmp<'a> {
    pub new: &'a BenchmarkStats,
    pub old: &'a BenchmarkStats,
}

impl BenchmarkCmp<'_> {
    pub fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec!["name", "old", "new", "diff", "diff_ratio", "speedup"]);
        for cmp in self.all_cmps() {
            let diff_ratio = format!("{:.2}%", cmp.diff_ratio * 100f64);
            let speedup = format!("{:.2}x", cmp.speedup);
            let diff = format!("{:.2}", cmp.diff);
            let mut row = Row::new();
            row.add_cell(Cell::new(cmp.name));
            row.add_cell(Cell::new(cmp.old_value));
            row.add_cell(Cell::new(cmp.new_value));
            if cmp.speedup >= 1.0 {
                row.add_cell(Cell::new(diff).fg(Color::Green));
                row.add_cell(Cell::new(diff_ratio).fg(Color::Green));
                row.add_cell(Cell::new(speedup).fg(Color::Green));
            } else {
                row.add_cell(Cell::new(diff).fg(Color::Red));
                row.add_cell(Cell::new(diff_ratio).fg(Color::Red));
                row.add_cell(Cell::new(speedup).fg(Color::Red));
            }
            table.add_row(row);
        }
        table
    }
    pub fn all_cmps(&self) -> Vec<Comparison> {
        vec![
            self.cmp_tps(),
            self.cmp_error_rate(),
            self.cmp_min_latency(),
            self.cmp_p25_latency(),
            self.cmp_p50_latency(),
            self.cmp_p75_latency(),
            self.cmp_p90_latency(),
            self.cmp_p99_latency(),
            self.cmp_p999_latency(),
            self.cmp_max_latency(),
        ]
    }
    pub fn cmp_tps(&self) -> Comparison {
        let old_tps = self.old.num_success_txes / self.old.duration.as_secs();
        let new_tps = self.new.num_success_txes / self.new.duration.as_secs();
        let diff = new_tps as i64 - old_tps as i64;
        let diff_ratio = diff as f64 / old_tps as f64;
        let speedup = 1.0 + diff_ratio;
        Comparison {
            name: "tps".to_string(),
            old_value: format!("{:.2}", old_tps),
            new_value: format!("{:.2}", new_tps),
            diff,
            diff_ratio,
            speedup,
        }
    }
    pub fn cmp_error_rate(&self) -> Comparison {
        let old_error_rate =
            self.old.num_error_txes / (self.old.num_error_txes + self.old.num_success_txes);
        let new_error_rate =
            self.new.num_error_txes / (self.new.num_error_txes + self.new.num_success_txes);
        let diff = new_error_rate as i64 - old_error_rate as i64;
        let diff_ratio = diff as f64 / old_error_rate as f64;
        let speedup = 1.0 / (1.0 + diff_ratio);
        Comparison {
            name: "error_rate".to_string(),
            old_value: format!("{:.2}", old_error_rate),
            new_value: format!("{:.2}", new_error_rate),
            diff,
            diff_ratio,
            speedup,
        }
    }
    pub fn cmp_min_latency(&self) -> Comparison {
        let old = self.old.latency_ms.histogram.min() as i64;
        let new = self.new.latency_ms.histogram.min() as i64;
        let diff = new - old;
        let diff_ratio = diff as f64 / old as f64;
        let speedup = 1.0 / (1.0 + diff_ratio);
        Comparison {
            name: "min_latency".to_string(),
            old_value: format!("{:.2}", old),
            new_value: format!("{:.2}", new),
            diff,
            diff_ratio,
            speedup,
        }
    }
    pub fn cmp_p25_latency(&self) -> Comparison {
        let old = self.old.latency_ms.histogram.value_at_quantile(0.25) as i64;
        let new = self.new.latency_ms.histogram.value_at_quantile(0.25) as i64;
        let diff = new - old;
        let diff_ratio = diff as f64 / old as f64;
        let speedup = 1.0 / (1.0 + diff_ratio);
        Comparison {
            name: "p25_latency".to_string(),
            old_value: format!("{:.2}", old),
            new_value: format!("{:.2}", new),
            diff,
            diff_ratio,
            speedup,
        }
    }
    pub fn cmp_p50_latency(&self) -> Comparison {
        let old = self.old.latency_ms.histogram.value_at_quantile(0.5) as i64;
        let new = self.new.latency_ms.histogram.value_at_quantile(0.5) as i64;
        let diff = new - old;
        let diff_ratio = diff as f64 / old as f64;
        let speedup = 1.0 / (1.0 + diff_ratio);
        Comparison {
            name: "p50_latency".to_string(),
            old_value: format!("{:.2}", old),
            new_value: format!("{:.2}", new),
            diff,
            diff_ratio,
            speedup,
        }
    }
    pub fn cmp_p75_latency(&self) -> Comparison {
        let old = self.old.latency_ms.histogram.value_at_quantile(0.75) as i64;
        let new = self.new.latency_ms.histogram.value_at_quantile(0.75) as i64;
        let diff = new - old;
        let diff_ratio = diff as f64 / old as f64;
        let speedup = 1.0 / (1.0 + diff_ratio);
        Comparison {
            name: "p75_latency".to_string(),
            old_value: format!("{:.2}", old),
            new_value: format!("{:.2}", new),
            diff,
            diff_ratio,
            speedup,
        }
    }
    pub fn cmp_p90_latency(&self) -> Comparison {
        let old = self.old.latency_ms.histogram.value_at_quantile(0.9) as i64;
        let new = self.new.latency_ms.histogram.value_at_quantile(0.9) as i64;
        let diff = new - old;
        let diff_ratio = diff as f64 / old as f64;
        let speedup = 1.0 / (1.0 + diff_ratio);
        Comparison {
            name: "p90_latency".to_string(),
            old_value: format!("{:.2}", old),
            new_value: format!("{:.2}", new),
            diff,
            diff_ratio,
            speedup,
        }
    }
    pub fn cmp_p99_latency(&self) -> Comparison {
        let old = self.old.latency_ms.histogram.value_at_quantile(0.99) as i64;
        let new = self.new.latency_ms.histogram.value_at_quantile(0.99) as i64;
        let diff = new - old;
        let diff_ratio = diff as f64 / old as f64;
        let speedup = 1.0 / (1.0 + diff_ratio);
        Comparison {
            name: "p99_latency".to_string(),
            old_value: format!("{:.2}", old),
            new_value: format!("{:.2}", new),
            diff,
            diff_ratio,
            speedup,
        }
    }
    pub fn cmp_p999_latency(&self) -> Comparison {
        let old = self.old.latency_ms.histogram.value_at_quantile(0.999) as i64;
        let new = self.new.latency_ms.histogram.value_at_quantile(0.999) as i64;
        let diff = new - old;
        let diff_ratio = diff as f64 / old as f64;
        let speedup = 1.0 / (1.0 + diff_ratio);
        Comparison {
            name: "p999_latency".to_string(),
            old_value: format!("{:.2}", old),
            new_value: format!("{:.2}", new),
            diff,
            diff_ratio,
            speedup,
        }
    }
    pub fn cmp_max_latency(&self) -> Comparison {
        let old = self.old.latency_ms.histogram.max() as i64;
        let new = self.new.latency_ms.histogram.max() as i64;
        let diff = new - old;
        let diff_ratio = diff as f64 / old as f64;
        let speedup = 1.0 / (1.0 + diff_ratio);
        Comparison {
            name: "max_latency".to_string(),
            old_value: format!("{:.2}", old),
            new_value: format!("{:.2}", new),
            diff,
            diff_ratio,
            speedup,
        }
    }
}

/// Convert an unsigned number into a string separated by `delim` every `step_size` digits
/// For example used to make 100000 more readable as 100,000
fn format_num_with_separators<T: Into<u128> + std::fmt::Display>(
    x: T,
    step_size: u8,
    delim: &'static str,
) -> String {
    x.to_string()
        .as_bytes()
        .rchunks(step_size as usize)
        .rev()
        .map(std::str::from_utf8)
        .collect::<Result<Vec<&str>, _>>()
        .unwrap()
        .join(delim)
}
