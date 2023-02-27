use std::time::Duration;

use color_eyre::eyre::{Context, Result};
use sui_benchmark::orchestrator::{
    client::VultrClient, settings::Settings, testbed::Testbed, Benchmark, BenchmarkParameters,
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

    let parameters = BenchmarkParameters {
        nodes: 10,
        faults: 0,
        load: 600,
        duration: Duration::from_secs(120),
    };

    let testbed = Testbed::new(settings, client)
        .await
        .wrap_err("Failed to crate testbed")?;

    testbed.info();

    let orchestrator = Orchestrator::new(testbed);
    orchestrator
        .run_benchmarks(vec![Benchmark::new(parameters)])
        .await
        .wrap_err("Failed to run benchmark")?;

    // testbed
    //     .populate(2)
    //     .await
    //     .wrap_err("Failed to populate tested")?;

    // testbed
    //     .install()
    //     .await
    //     .wrap_err("Failed to install software on instances")?;

    // testbed.kill(true).await.wrap_err("Failed to kill tested")?;

    // testbed
    //     .run_benchmark(&parameters)
    //     .await
    //     .wrap_err("Failed to deploy instances")?;

    // testbed
    //     .destroy()
    //     .await
    //     .wrap_err("Failed to destroy tested")?;

    // testbed.kill(true).await.wrap_err("Failed to kill tested")?;

    // testbed.stop().await.wrap_err("Failed to stop tested")?;

    Ok(())
}
