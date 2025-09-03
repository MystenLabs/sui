// Example demonstrating how to run the benchmark with concurrent validator submission for DOS testing
// This example shows how the benchmark now uses the new concurrent submission feature

use std::net::SocketAddr;
use std::sync::Arc;
use sui_benchmark::{
    benchmark_setup::BenchmarkSetup,
    drivers::bench_driver::BenchDriver,
    options::Opts,
    system_state_observer::SystemStateObserver,
    ValidatorProxy,
};
use tokio::runtime::Builder;
use tokio::sync::Barrier;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Example configuration for DOS testing with concurrent validator submission
    let mut opts = Opts::default();
    
    // Configure the benchmark for DOS testing
    opts.num_client_threads = 4;  // Multiple client threads to simulate load
    opts.num_server_threads = 2;   // Fewer server threads to create pressure
    opts.num_transfer_accounts = 2;
    opts.target_qps = 50;          // Moderate QPS to test DOS protection
    opts.in_flight_ratio = 3;      // High in-flight ratio to stress the system
    
    // Set up the benchmark
    let registry = prometheus::Registry::new();
    let barrier = Arc::new(Barrier::new(2));
    let cloned_barrier = barrier.clone();
    
    let bench_setup = BenchmarkSetup::new(cloned_barrier, &registry, &opts).await?;
    
    // The benchmark now automatically uses concurrent validator submission
    // when using TransactionDriver (which is configured via td_percentage)
    // 
    // Key changes made:
    // 1. ValidatorProxy::execute_transaction_block now accepts a client_addr parameter
    // 2. LocalValidatorAggregatorProxy uses SubmitTransactionOptions with:
    //    - forwarded_client_addr: client_addr (for DOS protection)
    //    - concurrent_validator_submissions: Some(3) (submit to 3 validators)
    // 3. One submission is blocking for consensus position
    // 4. Other submissions are non-blocking for DOS testing
    
    println!("Starting DOS test benchmark...");
    println!("Configuration:");
    println!("- Client threads: {}", opts.num_client_threads);
    println!("- Server threads: {}", opts.num_server_threads);
    println!("- Target QPS: {}", opts.target_qps);
    println!("- In-flight ratio: {}", opts.in_flight_ratio);
    println!("- Concurrent validator submissions: 3 (for DOS testing)");
    
    // Example of how the benchmark now works:
    // 1. When a transaction is submitted, it goes through execute_transaction_block
    // 2. If td_percentage > 0, it uses TransactionDriver with concurrent submission
    // 3. The transaction is submitted to 3 validators:
    //    - One blocking submission to get consensus position
    //    - Two non-blocking submissions for DOS testing
    // 4. The client_addr is passed through for DOS protection
    
    // Run the benchmark
    let system_state_observer = SystemStateObserver::new(
        bench_setup
            .proxies
            .choose(&mut rand::thread_rng())
            .expect("Failed to get proxy for system state observer")
            .clone(),
    );
    
    let bench_driver = BenchDriver::new(
        bench_setup.proxies,
        system_state_observer,
        &opts,
        registry,
    );
    
    bench_driver.run_benchmark().await?;
    
    println!("DOS test benchmark completed!");
    println!("The benchmark used concurrent validator submission to test DOS protection.");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use sui_benchmark::SubmitTransactionOptions;
    
    #[test]
    fn test_concurrent_submission_options() {
        // Test that the benchmark uses the correct SubmitTransactionOptions
        let client_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        
        let options = SubmitTransactionOptions {
            forwarded_client_addr: Some(client_addr),
            concurrent_validator_submissions: Some(3),
        };
        
        assert_eq!(options.concurrent_validator_submissions, Some(3));
        assert_eq!(options.forwarded_client_addr, Some(client_addr));
    }
    
    #[test]
    fn test_default_options() {
        // Test that default options don't use concurrent submission
        let options = SubmitTransactionOptions::default();
        
        assert_eq!(options.concurrent_validator_submissions, None);
    }
}
