use std::time::Duration;

use color_eyre::eyre::{Context, Result};
use sui_benchmark::orchestrator::{
    client::VultrClient,
    parameters::{BenchmarkParametersGenerator, LoadType},
    settings::Settings,
    testbed::Testbed,
    Orchestrator,
};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let path = "crates/sui-benchmark/src/orchestrator/assets/settings.json";
    let settings = Settings::load(path).wrap_err("Failed to load settings")?;

    let token = settings.load_token()?;
    let client = VultrClient::new(token, settings.clone());

    let public_key = settings.load_ssh_public_key()?;
    client.upload_key(public_key).await?;

    let testbed = Testbed::new(settings, client)
        .await
        .wrap_err("Failed to crate testbed")?;

    testbed.info();

    let nodes = 10;
    let parameters_generator = BenchmarkParametersGenerator::new(
        nodes,
        LoadType::Search {
            starting_load: 600,
            latency_increase_tolerance: 2,
            max_iterations: 5,
        },
    )
    .with_custom_duration(Duration::from_secs(120));

    Orchestrator::new(testbed, parameters_generator)
        .do_not_update_testbed()
        // .do_not_reconfigure_testbed()
        .run_benchmarks()
        .await
        .wrap_err("Failed to run benchmark")?;

    Ok(())
}
