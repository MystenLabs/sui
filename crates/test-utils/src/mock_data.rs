// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mock data generators for testing Sui components

use rand::Rng;

/// Generate a random address string
pub fn random_address() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..20).map(|_| rng.gen()).collect();
    format!("0x{}", hex::encode(bytes))
}

/// Generate a random object ID
pub fn random_object_id() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    hex::encode(bytes)
}

/// Generate a random transaction digest
pub fn random_digest() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    hex::encode(bytes)
}

/// Generate multiple random addresses
pub fn random_addresses(count: usize) -> Vec<String> {
    (0..count).map(|_| random_address()).collect()
}

/// Generate multiple random object IDs
pub fn random_object_ids(count: usize) -> Vec<String> {
    (0..count).map(|_| random_object_id()).collect()
}

/// Mock address generator with predictable patterns
pub struct MockAddressGenerator {
    counter: u64,
    prefix: String,
}

impl MockAddressGenerator {
    /// Create a new generator
    pub fn new() -> Self {
        Self {
            counter: 0,
            prefix: String::from("0x"),
        }
    }

    /// Create a generator with a custom prefix
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            counter: 0,
            prefix: prefix.into(),
        }
    }

    /// Generate the next address
    pub fn next(&mut self) -> String {
        let addr = format!("{}{:040x}", self.prefix, self.counter);
        self.counter += 1;
        addr
    }

    /// Generate N addresses
    pub fn next_n(&mut self, count: usize) -> Vec<String> {
        (0..count).map(|_| self.next()).collect()
    }

    /// Reset the counter
    pub fn reset(&mut self) {
        self.counter = 0;
    }
}

impl Default for MockAddressGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock transaction data
pub struct MockTransaction {
    pub sender: String,
    pub recipient: String,
    pub amount: u64,
    pub digest: String,
    pub gas_used: u64,
}

impl MockTransaction {
    /// Create a new mock transaction
    pub fn new(sender: String, recipient: String, amount: u64) -> Self {
        Self {
            sender,
            recipient,
            amount,
            digest: random_digest(),
            gas_used: 1000,
        }
    }

    /// Create a random mock transaction
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            sender: random_address(),
            recipient: random_address(),
            amount: rng.gen_range(1..1_000_000),
            digest: random_digest(),
            gas_used: rng.gen_range(100..10_000),
        }
    }

    /// Create multiple random transactions
    pub fn random_batch(count: usize) -> Vec<Self> {
        (0..count).map(|_| Self::random()).collect()
    }
}

/// Mock object data
pub struct MockObject {
    pub id: String,
    pub owner: String,
    pub object_type: String,
    pub version: u64,
}

impl MockObject {
    /// Create a new mock object
    pub fn new(owner: String, object_type: String) -> Self {
        Self {
            id: random_object_id(),
            owner,
            object_type,
            version: 1,
        }
    }

    /// Create a random mock object
    pub fn random() -> Self {
        Self {
            id: random_object_id(),
            owner: random_address(),
            object_type: String::from("0x2::coin::Coin<0x2::sui::SUI>"),
            version: 1,
        }
    }

    /// Create multiple random objects
    pub fn random_batch(count: usize) -> Vec<Self> {
        (0..count).map(|_| Self::random()).collect()
    }

    /// Increment version
    pub fn increment_version(&mut self) {
        self.version += 1;
    }
}

/// Mock event data
pub struct MockEvent {
    pub event_type: String,
    pub sender: String,
    pub timestamp: u64,
    pub data: String,
}

impl MockEvent {
    /// Create a new mock event
    pub fn new(event_type: String, sender: String) -> Self {
        Self {
            event_type,
            sender,
            timestamp: current_timestamp(),
            data: String::from("{}"),
        }
    }

    /// Create a random mock event
    pub fn random() -> Self {
        Self {
            event_type: String::from("0x2::transfer::TransferEvent"),
            sender: random_address(),
            timestamp: current_timestamp(),
            data: String::from("{}"),
        }
    }

    /// Create multiple random events
    pub fn random_batch(count: usize) -> Vec<Self> {
        (0..count).map(|_| Self::random()).collect()
    }

    /// With custom data
    pub fn with_data(mut self, data: String) -> Self {
        self.data = data;
        self
    }
}

/// Get current timestamp in milliseconds
fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_address() {
        let addr = random_address();
        assert!(addr.starts_with("0x"));
        assert_eq!(addr.len(), 42); // 0x + 40 hex chars
    }

    #[test]
    fn test_address_generator() {
        let mut gen = MockAddressGenerator::new();
        let addr1 = gen.next();
        let addr2 = gen.next();

        assert_ne!(addr1, addr2);
        assert!(addr1.starts_with("0x"));
        assert!(addr2.starts_with("0x"));
    }

    #[test]
    fn test_mock_transaction() {
        let tx = MockTransaction::random();
        assert!(tx.sender.starts_with("0x"));
        assert!(tx.recipient.starts_with("0x"));
        assert!(tx.amount > 0);
        assert!(!tx.digest.is_empty());
    }

    #[test]
    fn test_mock_object() {
        let obj = MockObject::random();
        assert!(!obj.id.is_empty());
        assert!(obj.owner.starts_with("0x"));
        assert_eq!(obj.version, 1);
    }

    #[test]
    fn test_random_batch() {
        let txs = MockTransaction::random_batch(5);
        assert_eq!(txs.len(), 5);

        let objs = MockObject::random_batch(3);
        assert_eq!(objs.len(), 3);
    }
}
