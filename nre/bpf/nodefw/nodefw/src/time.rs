// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use nix::time::{clock_gettime, ClockId};
use std::time::Duration;

pub fn get_ktime_get_ns() -> u64 {
    // we can't use a u128, that's crazy, we lose precision but we don't have it from the
    // kernel anyway (easily in bpf)
    Duration::from(clock_gettime(ClockId::CLOCK_BOOTTIME).unwrap()).as_nanos() as u64
}

pub fn ttl(v: Duration) -> u64 {
    let start = Duration::from(clock_gettime(ClockId::CLOCK_BOOTTIME).unwrap());
    (start + v).as_nanos() as u64
}
