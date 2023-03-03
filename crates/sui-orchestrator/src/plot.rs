use std::{collections::HashMap, fmt::Debug, path::PathBuf, time::Duration};

use glob::glob;
use plotters::{
    prelude::{BitMapBackend, ChartBuilder, ErrorBar, IntoDrawingArea},
    series::LineSeries,
    style::{Color, RED, WHITE},
};

use crate::{measurement::MeasurementsCollection, settings::Settings};

pub struct MeasurementsCollectionSummary {
    average_tps: u64,
    average_latency: Duration,
    stdev_latency: Duration,
}

impl From<&MeasurementsCollection> for MeasurementsCollectionSummary {
    fn from(collection: &MeasurementsCollection) -> Self {
        Self {
            average_tps: collection.aggregate_tps(),
            average_latency: collection.aggregate_average_latency(),
            stdev_latency: collection.aggregate_stdev_latency(),
        }
    }
}

#[derive(Hash, PartialEq, Eq)]
pub struct MeasurementsCollectionId {
    nodes: usize,
    faults: usize,
    duration: Duration,
    machine_specs: String,
}

impl Debug for MeasurementsCollectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}-{}",
            self.faults,
            self.nodes,
            self.duration.as_secs()
        )
    }
}

impl From<MeasurementsCollection> for MeasurementsCollectionId {
    fn from(collection: MeasurementsCollection) -> Self {
        Self {
            nodes: collection.parameters.nodes,
            faults: collection.parameters.faults,
            duration: collection.parameters.duration,
            machine_specs: collection.machine_specs,
        }
    }
}

pub struct Plotter {
    settings: Settings,
    measurements: HashMap<MeasurementsCollectionId, Vec<MeasurementsCollection>>,
}

impl Plotter {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            measurements: HashMap::new(),
        }
    }

    pub fn collect_measurements(mut self) -> Self {
        let mut path = self.settings.results_directory.clone();
        path.push("*");
        path.set_extension("json");

        if let Ok(files) = glob(&path.display().to_string()) {
            for file in files {
                if let Ok(file) = file {
                    match MeasurementsCollection::load(&file) {
                        Ok(measurement) => {
                            let setup = measurement.clone().into();
                            self.measurements
                                .entry(setup)
                                .or_insert_with(Vec::new)
                                .push(measurement);
                        }
                        Err(e) => println!("skipping {file:?}: {e}"),
                    }
                }
            }
        }
        self
    }

    pub fn plot_latency_throughput(&self) -> Result<(), Box<dyn std::error::Error>> {
        for (setup, collections) in &self.measurements {
            let mut sorted = collections.clone();
            sorted.sort_by(|a, b| a.parameters.load.cmp(&b.parameters.load));

            let data_points = sorted
                .iter()
                .map(|collection| {
                    let summary: MeasurementsCollectionSummary = collection.into();
                    (
                        summary.average_tps as f32,
                        summary.average_latency.as_secs_f64() as f32,
                        summary.stdev_latency.as_secs_f64() as f32,
                    )
                })
                .collect();

            self.plot_impl(setup, data_points)?;
        }
        Ok(())
    }

    fn plot_impl(
        &self,
        setup: &MeasurementsCollectionId,
        data_points: Vec<(f32, f32, f32)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut filename = PathBuf::new();
        filename.push(&self.settings.results_directory);
        filename.push(format!("latency-{setup:?}"));
        // filename.set_extension("svg");
        filename.set_extension("png");

        // let root = SVGBackend::new(&filename, (640, 480)).into_drawing_area();
        let root = BitMapBackend::new(&filename, (640, 320)).into_drawing_area();
        root.fill(&WHITE)?;
        let root = root.margin(10, 10, 10, 10);

        let mut chart = ChartBuilder::on(&root)
            .x_label_area_size(20)
            .y_label_area_size(20)
            .build_cartesian_2d(0.0..1_500f32, 0.0..1.5f32)?;

        chart
            .configure_mesh()
            .x_labels(5)
            .y_labels(5)
            .x_label_formatter(&|x| format!("{}", x))
            .y_label_formatter(&|x| format!("{}", x))
            .draw()?;

        chart.draw_series(data_points.iter().map(|(x, y, std)| {
            let yl = (y - std).max(0.0);
            let yh = y + std;
            ErrorBar::new_vertical(*x, yl, *y, yh, RED.filled(), 10)
        }))?;

        chart.draw_series(LineSeries::new(
            data_points.iter().map(|(x, y, _)| (*x, *y)),
            &RED,
        ))?;

        root.present()?;
        Ok(())
    }
}
