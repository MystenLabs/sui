use std::time::Duration;
use test_utils::network::TestClusterBuilder;

#[tokio::main]
async fn main() {
    let _cluster = TestClusterBuilder::new().build().await;
    tokio::time::sleep(Duration::from_secs(3)).await;
}
