// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// Error types
#[macro_use]
pub mod error;

mod consensus;
pub use consensus::*;

mod primary;
pub use primary::*;

mod proto;
pub use proto::*;

mod worker;
pub use worker::*;

mod serde;

pub mod bounded_future_queue;
pub mod metered_channel;

#[macro_export]
macro_rules! random_state_log {
    () => {{
        use std::hash::BuildHasher;
        use std::hash::Hash;
        use std::hash::Hasher;

        let s = std::collections::hash_map::RandomState::new();
        let mut h1 = s.build_hasher();
        1_u64.hash(&mut h1);
        let random_state = h1.finish();
        dbg!(random_state);
    }};
}
