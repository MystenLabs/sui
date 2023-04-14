// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    path::PathBuf,
    time::Duration,
};

use color_eyre::eyre::{Context, Result};
use glob::glob;
use plotters::{
    prelude::{BitMapBackend, ChartBuilder, ErrorBar, IntoDrawingArea},
    series::LineSeries,
    style::{Color, RED, WHITE},
};

use crate::{
    benchmark::BenchmarkType, faults::FaultsType, measurement::MeasurementsCollection,
    settings::Settings,
};

/// The set of parameters that uniquely identify a set of measurements. This id avoids
/// plotting incomparable measurements on the same graph.
#[derive(Hash, PartialEq, Eq)]
pub struct MeasurementsCollectionId<T> {
    benchmark_type: T,
    nodes: usize,
    faults: FaultsType,
    duration: Duration,
    machine_specs: String,
    commit: String,
}

impl<T: BenchmarkType> Debug for MeasurementsCollectionId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{:?}-{}-{}",
            self.benchmark_type,
            self.faults,
            self.nodes,
            self.duration.as_secs()
        )
    }
}

impl<T: BenchmarkType> Display for MeasurementsCollectionId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} nodes ({}) - {}% shared objects",
            self.nodes, self.faults, self.benchmark_type
        )
    }
}

impl<T> From<MeasurementsCollection<T>> for MeasurementsCollectionId<T> {
    fn from(collection: MeasurementsCollection<T>) -> Self {
        Self {
            benchmark_type: collection.parameters.benchmark_type,
            nodes: collection.parameters.nodes,
            faults: collection.parameters.faults,
            duration: collection.parameters.duration,
            machine_specs: collection.machine_specs,
            commit: collection.commit,
        }
    }
}

/// A data point of the plot.
struct PlotDataPoint {
    /// The x coordinate.
    x: f32,
    /// The y coordinate.
    y: f32,
    /// The y-stdev to plot as error bars.
    stdev: f32,
}

impl<T: BenchmarkType> From<&MeasurementsCollection<T>> for PlotDataPoint {
    fn from(collection: &MeasurementsCollection<T>) -> Self {
        Self {
            x: collection.aggregate_tps() as f32,
            y: collection.aggregate_average_latency().as_secs_f64() as f32,
            stdev: collection.aggregate_stdev_latency().as_secs_f64() as f32,
        }
    }
}

/// Plot latency-throughput graphs.
pub struct Plotter<T> {
    /// The benchmarks settings.
    settings: Settings,
    /// The collection of measurements to plot.
    measurements: HashMap<MeasurementsCollectionId<T>, Vec<MeasurementsCollection<T>>>,
    /// The limit of the x-axis.
    x_lim: Option<f32>,
    /// THe limit of the y-axis.
    y_lim: Option<f32>,
}

impl<T: BenchmarkType> Plotter<T> {
    /// Make a new plotter from the benchmarks settings.
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            measurements: HashMap::new(),
            x_lim: None,
            y_lim: None,
        }
    }

    /// Set the limit of the x-axis.
    pub fn with_x_lim(mut self, x_lim: Option<f32>) -> Self {
        self.x_lim = x_lim;
        self
    }

    /// Set the limit of the y-axis.
    pub fn with_y_lim(mut self, y_lim: Option<f32>) -> Self {
        self.y_lim = y_lim;
        self
    }

    /// Load all possible measurements from the results directory specified in the settings.
    pub fn load_measurements(mut self) -> Self {
        let mut path = self.settings.results_dir.clone();
        path.push("*");
        path.set_extension("json");

        if let Ok(files) = glob(&path.display().to_string()) {
            for file in files.flatten() {
                match MeasurementsCollection::load(&file) {
                    Ok(measurement) => {
                        let id = measurement.clone().into();
                        self.measurements
                            .entry(id)
                            .or_insert_with(Vec::new)
                            .push(measurement);
                    }
                    Err(e) => println!("skipping {file:?}: {e}"),
                }
            }
        }
        self
    }

    /// Plot a latency-throughput graphs.
    pub fn plot_latency_throughput(&self) -> Result<()> {
        for (id, collections) in &self.measurements {
            let mut sorted = collections.clone();
            sorted.sort_by(|a, b| a.parameters.load.cmp(&b.parameters.load));

            let data_points = sorted.iter().map(|collection| collection.into()).collect();
            self.plot_impl(id, data_points)
                .wrap_err(format!("Failed to plot measurements id {id}"))?;
        }
        Ok(())
    }

    fn plot_impl(
        &self,
        id: &MeasurementsCollectionId<T>,
        data_points: Vec<PlotDataPoint>,
    ) -> Result<()> {
        // Set the directory to save plots and compute the plot's filename.
        let mut filename = PathBuf::new();
        filename.push(&self.settings.results_dir);
        filename.push("plots");
        std::fs::create_dir_all(&filename)?;
        filename.push(format!("latency-{id:?}"));
        filename.set_extension("png");

        // Prepare the plot frame.
        let root = BitMapBackend::new(&filename, (640, 320)).into_drawing_area();
        root.fill(&WHITE)?;
        let root = root.margin(10, 10, 10, 10);

        let x_lim = self.x_lim.unwrap_or_else(|| {
            (data_points
                .iter()
                .map(|data| (data.x * 100.0) as u64)
                .max()
                .unwrap_or_default()
                / 100) as f32
        });
        let y_lim = self.y_lim.unwrap_or_else(|| {
            (data_points
                .iter()
                .map(|data| (data.y * 100.0) as u64)
                .max()
                .unwrap_or_default()
                / 100) as f32
        });
        let mut chart = ChartBuilder::on(&root)
            .caption(format!("{id}"), ("sans-serif", 20))
            .x_label_area_size(30)
            .y_label_area_size(30)
            .build_cartesian_2d(0.0..x_lim, 0.0..y_lim)?;

        // Configure the axis.
        chart
            .configure_mesh()
            .x_desc("\nThroughput (tx/s)")
            .y_desc("\nLatency (s)")
            .x_label_formatter(&|x| format!("{}", x))
            .y_label_formatter(&|x| format!("{}", x))
            .draw()?;

        // Draw lines and error bars between data points.
        chart.draw_series(data_points.iter().map(|data| {
            let yl = (data.y - data.stdev).max(0.0);
            let yh = data.y + data.stdev;
            ErrorBar::new_vertical(data.x, yl, data.y, yh, RED.filled(), 10)
        }))?;

        // Plot the measurements points.
        chart.draw_series(LineSeries::new(
            data_points.iter().map(|data| (data.x, data.y)),
            RED,
        ))?;

        // Save the plot to file.
        root.present()?;
        Ok(())
    }
}
