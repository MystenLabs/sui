// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! lock_service is a single-threaded atomic Sui Object locking service.
//! Object locks have three phases:
//! 1. (object has no lock, doesn't exist)
//! 2. None (object has an empty lock, but exists. The state when a new object is created)
//! 3. Locked (object has a Transaction digest in the lock, so it's only usable by that transaction)
//!
//! The cycle goes from None (object creation) -> Locked -> deleted/doesn't exist after a Transaction.
//!
//! Lock state is persisted in RocksDB and should be consistent.
//!
//! Communication with the lock service happens through a MPSC queue/channel.

pub mod lock_service;

pub use lock_service::LockService;
