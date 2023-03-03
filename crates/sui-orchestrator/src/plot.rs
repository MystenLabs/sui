use std::{collections::HashMap, fmt::Debug, path::PathBuf, time::Duration};

use glob::glob;
use plotters::{
    prelude::{BitMapBackend, ChartBuilder, ErrorBar, IntoDrawingArea},
    series::LineSeries,
    style::{Color, RED, WHITE},
};

use crate::{measurement::MeasurementsCollection, settings::Settings};

#[derive(Hash, PartialEq, Eq)]
pub struct MeasurementsCollectionId {
    shared_objects_ratio: u16,
    nodes: usize,
    faults: usize,
    duration: Duration,
    machine_specs: String,
}

impl Debug for MeasurementsCollectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}",
            self.shared_objects_ratio,
            self.faults,
            self.nodes,
            self.duration.as_secs()
        )
    }
}

impl From<MeasurementsCollection> for MeasurementsCollectionId {
    fn from(collection: MeasurementsCollection) -> Self {
        Self {
            shared_objects_ratio: collection.parameters.shared_objects_ratio,
            nodes: collection.parameters.nodes,
            faults: collection.parameters.faults,
            duration: collection.parameters.duration,
            machine_specs: collection.machine_specs,
        }
    }
}

struct PlotDataPoint {
    x: f32,
    y: f32,
    stdev: f32,
}

impl From<&MeasurementsCollection> for PlotDataPoint {
    fn from(collection: &MeasurementsCollection) -> Self {
        Self {
            x: collection.aggregate_tps() as f32,
            y: collection.aggregate_average_latency().as_secs_f64() as f32,
            stdev: collection.aggregate_stdev_latency().as_secs_f64() as f32,
        }
    }
}

pub struct Plotter {
    settings: Settings,
    measurements: HashMap<MeasurementsCollectionId, Vec<MeasurementsCollection>>,
    x_lim: Option<f32>,
    y_lim: Option<f32>,
}

impl Plotter {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            measurements: HashMap::new(),
            x_lim: None,
            y_lim: None,
        }
    }

    pub fn with_x_lim(mut self, x_lim: Option<f32>) -> Self {
        self.x_lim = x_lim;
        self
    }

    pub fn with_y_lim(mut self, y_lim: Option<f32>) -> Self {
        self.y_lim = y_lim;
        self
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
        }
        self
    }

    pub fn plot_latency_throughput(&self) -> Result<(), Box<dyn std::error::Error>> {
        for (id, collections) in &self.measurements {
            let mut sorted = collections.clone();
            sorted.sort_by(|a, b| a.parameters.load.cmp(&b.parameters.load));

            let data_points = sorted.iter().map(|collection| collection.into()).collect();

            self.plot_impl(id, data_points)?;
        }
        Ok(())
    }

    fn plot_impl(
        &self,
        id: &MeasurementsCollectionId,
        data_points: Vec<PlotDataPoint>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut filename = PathBuf::new();
        filename.push(&self.settings.results_directory);
        filename.push(format!("latency-{id:?}"));
        // filename.set_extension("svg");
        filename.set_extension("png");

        // let root = SVGBackend::new(&filename, (640, 480)).into_drawing_area();
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
            .x_label_area_size(20)
            .y_label_area_size(20)
            .build_cartesian_2d(0.0..x_lim, 0.0..y_lim)?;

        chart
            .configure_mesh()
            .x_labels(5)
            .y_labels(5)
            .x_label_formatter(&|x| format!("{}", x))
            .y_label_formatter(&|x| format!("{}", x))
            .draw()?;

        chart.draw_series(data_points.iter().map(|data| {
            let yl = (data.y - data.stdev).max(0.0);
            let yh = data.y + data.stdev;
            ErrorBar::new_vertical(data.x, yl, data.y, yh, RED.filled(), 10)
        }))?;

        chart.draw_series(LineSeries::new(
            data_points.iter().map(|data| (data.x, data.y)),
            &RED,
        ))?;

        root.present()?;
        Ok(())
    }
}
