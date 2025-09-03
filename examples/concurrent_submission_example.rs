// Example demonstrating how to use concurrent validator submission for DOS protection testing
// This example shows how to configure the SubmitTransactionOptions to submit to multiple validators

use sui_core::transaction_driver::SubmitTransactionOptions;
use std::net::SocketAddr;

fn main() {
    // Example 1: Submit to 3 validators concurrently
    // One submission will be blocking to get consensus position
    // The other 2 will be non-blocking
    let options_with_concurrent = SubmitTransactionOptions {
        forwarded_client_addr: Some("127.0.0.1:8080".parse::<SocketAddr>().unwrap()),
        concurrent_validator_submissions: Some(3),
    };

    // Example 2: Default behavior - submit to one validator at a time
    let options_default = SubmitTransactionOptions {
        forwarded_client_addr: Some("127.0.0.1:8080".parse::<SocketAddr>().unwrap()),
        concurrent_validator_submissions: None,
    };

    // Example 3: Submit to 5 validators for aggressive DOS protection testing
    let options_aggressive = SubmitTransactionOptions {
        forwarded_client_addr: Some("127.0.0.1:8080".parse::<SocketAddr>().unwrap()),
        concurrent_validator_submissions: Some(5),
    };

    println!("Concurrent submission examples:");
    println!("- Default behavior: {:?}", options_default.concurrent_validator_submissions);
    println!("- 3 validators: {:?}", options_with_concurrent.concurrent_validator_submissions);
    println!("- 5 validators (aggressive): {:?}", options_aggressive.concurrent_validator_submissions);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concurrent_submission_options() {
        let options = SubmitTransactionOptions {
            forwarded_client_addr: None,
            concurrent_validator_submissions: Some(3),
        };
        
        assert_eq!(options.concurrent_validator_submissions, Some(3));
    }

    #[test]
    fn test_default_submission_options() {
        let options = SubmitTransactionOptions::default();
        
        assert_eq!(options.concurrent_validator_submissions, None);
    }
}
