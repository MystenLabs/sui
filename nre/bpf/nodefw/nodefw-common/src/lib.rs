// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![no_std]
// https://github.com/aya-rs/aya/blob/79c1d8495e49acea45a952170acbbd41a8cb6485/aya/src/bpf.rs#L265C1-L267C67
// The type of a global variable must be `Pod` (plain old data), for instance `u8`, `u32` and
// all other primitive types. You may use custom types as well, but you must ensure that those
// types are `#[repr(C)]` and only contain other `Pod` types.
//
// within the context of this no_std struct, that means we can use a custom struct in maps and what not
// but we must ensure the fields are pod type files. nothing crazy.

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Rule {
    pub ttl: u64,
    pub port: u16,
}

// the feature gate is needed to make it work for no_std and std
#[cfg(feature = "user")]
unsafe impl aya::Pod for Rule {}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Meta {
    pub ktime: u64,
}

#[cfg(feature = "user")]
unsafe impl aya::Pod for Meta {}
