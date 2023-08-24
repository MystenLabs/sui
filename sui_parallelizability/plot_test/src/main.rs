use charming::{
    component::{Axis, Legend},
    element::{AxisType, LineStyle, Color, AreaStyle, ColorStop, ItemStyle, MarkLine, Symbol, Label, MarkLineVariant, MarkLineData, TextStyle, NameLocation},
    series::Line,
    Chart, ImageRenderer
};
use polars::{prelude::*, export::arrow::Either};
use polars::datatypes::DataType;

/*fn main() {
    let chart = Chart::new()
        .x_axis(
            Axis::new()
                .name("Number of nodes")
                .name_location(NameLocation::Center)
                .name_gap(24)
                .name_text_style(TextStyle::new().font_size(14))
                //.type_(AxisType::Log)
        )
        .y_axis(
            Axis::new()
                .name("Adversary stake (%)")
                .name_location(NameLocation::Center)
                .name_gap(32)
                .name_text_style(TextStyle::new().font_size(14))
        )
        .series(
            Line::new()
                .line_style(LineStyle::new().color(Color::from("#4285f4")))
                .item_style(
                    ItemStyle::new()
                        .color(Color::from("#4285f4"))
                        .border_width(2)
                )
                /*.area_style(
                    AreaStyle::new()
                        .color(Color::LinearGradient {
                            x: 0.0,
                            y: 0.0,
                            x2: 0.0,
                            y2: 1.0,
                            color_stops: vec![
                                ColorStop::new(0.0, "#4285f4"),
                                ColorStop::new(1.0, "#ffffff"),
                            ],
                        })
                        .opacity(0.8),
                )
                .smooth(0.5)*/
                .data(df![[1000, 2.99], [2000, 1.96], [5000, 1.35], [10_000, 0.72], [25_000, 0.63], [50_000, 0.42]]),
        );

    let mut renderer = ImageRenderer::new(800, 400);
    renderer.save(&chart, "./avalanche.svg").unwrap();
}*/

fn main() {
    let df = load_data().unwrap();
    let chart = create_chart(df);

    let mut renderer = ImageRenderer::new(1200, 600);
    renderer.save(&chart, "../scheduling.svg").unwrap();
}

fn load_data() -> Result<DataFrame, PolarsError> {
    let path = "../data/scheduling.csv";
    let lazy = LazyCsvReader::new(path)
        .has_header(true)
        .finish()?
        .filter(col("sequential_gas").neq(0))
        .sort("epoch", Default::default())
        .with_column((col("total_gas").cast(DataType::Float64) / col("sequential_gas").cast(DataType::Float64)).alias("speedup"))
        .with_column((col("total_gas").cast(DataType::Float64) / col("max_tx_gas").cast(DataType::Float64)).alias("max_tx_speedup"))
        .with_column((col("total_gas").cast(DataType::Float64) / col("max_cc_gas").cast(DataType::Float64)).alias("max_cc_speedup"))
        .with_column(col("num_nodes").cast(DataType::Float64).alias("num_tx_speedup"))
        .select(&[col("epoch"), col("num_tx_speedup"), col("speedup"), col("max_tx_speedup"), col("max_cc_speedup"), col("total_gas").cast(DataType::Float64)])
        .groupby(&["epoch"])
        .agg([
            (col("max_cc_speedup").dot(col("total_gas")) / col("total_gas").sum()),
            (col("speedup").dot(col("total_gas")) / col("total_gas").sum()),
            (col("max_tx_speedup").dot(col("total_gas")) / col("total_gas").sum()),
            (col("num_tx_speedup").dot(col("total_gas")) / col("total_gas").sum()),
        ]);
        //.agg([col("num_nodes").mean(), col("speedup").mean(), col("max_speedup").mean()]);
    let df = lazy.collect()?;
    eprintln!("{:?}", df);
    Ok(df)
}

fn create_chart(df: DataFrame) -> Chart {
    let epochs = df.column("epoch").unwrap();
    let epochs = match epochs.i64().unwrap().to_vec_null_aware() {
        Either::Left(v) => v,
        Either::Right(_) => panic!("null values found, not expected"),
    };

    let num_nodes = df.column("num_tx_speedup").unwrap();
    let num_nodes = match num_nodes.f64().unwrap().to_vec_null_aware() {
        Either::Left(v) => v,
        Either::Right(_) => panic!("null values found, not expected"),
    };

    let speedups = df.column("speedup").unwrap();
    let speedups = match speedups.f64().unwrap().to_vec_null_aware() {
        Either::Left(v) => v,
        Either::Right(_) => panic!("null values found, not expected"),
    };

    let max_speedups = df.column("max_tx_speedup").unwrap();
    let max_speedups = match max_speedups.f64().unwrap().to_vec_null_aware() {
        Either::Left(v) => v,
        Either::Right(_) => panic!("null values found, not expected"),
    };

    let max_cc_speedups = df.column("max_cc_speedup").unwrap();
    let max_cc_speedups = match max_cc_speedups.f64().unwrap().to_vec_null_aware() {
        Either::Left(v) => v,
        Either::Right(_) => panic!("null values found, not expected"),
    };

    let mixed_df: charming::datatype::dataframe::DataFrame = epochs.clone().into_iter()
        .zip(speedups.into_iter())
        .map(|(epoch, speedup)| charming::datatype::datapoint::DataPoint::from(vec![epoch as f64, speedup]))
        .collect();

    let mixed_df_2: charming::datatype::dataframe::DataFrame = epochs.clone().into_iter()
        .zip(num_nodes.into_iter())
        .map(|(epoch, num_nodes)| charming::datatype::datapoint::DataPoint::from(vec![epoch as f64, num_nodes]))
        .collect();

    let mixed_df_3: charming::datatype::dataframe::DataFrame = epochs.clone().into_iter()
        .zip(max_speedups.into_iter())
        .map(|(epoch, max_speedup)| charming::datatype::datapoint::DataPoint::from(vec![epoch as f64, max_speedup]))
        .collect();

    let mixed_df_4: charming::datatype::dataframe::DataFrame = epochs.clone().into_iter()
        .zip(max_cc_speedups.into_iter())
        .map(|(epoch, max_cc_speedup)| charming::datatype::datapoint::DataPoint::from(vec![epoch as f64, max_cc_speedup]))
        .collect();

    Chart::new()
        .x_axis(
            Axis::new()
                .name("Epoch")
                .name_location(NameLocation::Center)
                .name_gap(16)
                .name_text_style(TextStyle::new().font_size(14))
        )
        .y_axis(
            Axis::new()
                .name("Possible speedup")
                .name_location(NameLocation::Center)
                .name_gap(24)
                .name_text_style(TextStyle::new().font_size(14))
                .type_(AxisType::Log)
        )
        .legend(Legend::new().top("top"))
        .series(
            Line::new()
                .name("number of Txs")
                .line_style(LineStyle::new().color(Color::from("#b6b6b6")))
                .show_symbol(false)
                .item_style(ItemStyle::new().opacity(0.0))
                .smooth(0.5)
                .data(mixed_df_2),
        )
        .series(
            Line::new()
                .name("heaviest Tx")
                .line_style(LineStyle::new().color(Color::from("#ee6699")))
                .show_symbol(false)
                .item_style(ItemStyle::new().opacity(0.0))
                .smooth(0.5)
                .data(mixed_df_3),
        )
        .series(
            Line::new()
                .name("longest sequential chain")
                .line_style(LineStyle::new().color(Color::from("#4285f4")))
                .item_style(
                    ItemStyle::new()
                        .color(Color::from("#4285f4"))
                        .border_width(2)
                )
                .area_style(
                    AreaStyle::new()
                        .color(Color::LinearGradient {
                            x: 0.0,
                            y: 0.0,
                            x2: 0.0,
                            y2: 1.0,
                            color_stops: vec![
                                ColorStop::new(0.0, "#4285f4"),
                                ColorStop::new(1.0, "#ffffff"),
                            ],
                        })
                        .opacity(0.8),
                )
                .smooth(0.5)
                .mark_line(
                    MarkLine::new()
                        .symbol(vec![Symbol::None, Symbol::None])
                        .label(Label::new().show(false))
                        .line_style(
                            LineStyle::new()
                                .width(2)
                                .color(Color::from("rgb(240, 23, 32)")))
                        .data(vec![
                            MarkLineVariant::Simple(MarkLineData::new().x_axis(20)),
                        ]),
                )
                .data(mixed_df),
        )
}
