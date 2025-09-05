// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests in here are based on binary representation of modules taken from production. Those tests
//! may fail over time if the representation becomes out of date, then they can be removed.
//! Right now the serve to calibrate the metering working as expected. Those tests represent
//! cases which we want to continue to succeed.

use crate::unit_tests::production_config;
use move_binary_format::{errors::VMResult, CompiledModule};
use move_bytecode_verifier::verifier;
use move_bytecode_verifier_meter::bound::BoundMeter;

#[allow(unused)]
fn run_binary_test(name: &str, bytes: &str) -> VMResult<()> {
    let bytes = hex::decode(bytes).expect("invalid hex string");
    let m = CompiledModule::deserialize_with_defaults(&bytes).expect("invalid module");
    let (verifier_config, meter_config) = production_config();
    let mut meter = BoundMeter::new(meter_config);
    verifier::verify_module_with_config_for_test(name, &verifier_config, &m, &mut meter)
}

macro_rules! do_test {
    ($name:expr) => {{
        let name = $name;
        let code = std::fs::read_to_string(format!("tests/binaries/{name}.bytes")).unwrap();
        let res = run_binary_test(name, &code);
        assert!(res.is_ok(), "{:?}", res)
    }};
}

#[test]
fn aptosd_swap() {
    do_test!("aptosd_swap");
}

#[test]
fn coin_store() {
    do_test!("coin_store");
}

#[test]
fn farming() {
    do_test!("farming");
}

#[test]
fn liquidity_pool() {
    do_test!("liquidity_pool");
}

#[test]
fn price_oracle() {
    do_test!("price_oracle");
}

#[test]
fn pool() {
    do_test!("pool");
}

#[test]
fn router() {
    do_test!("router");
}

#[test]
fn whitelist() {
    do_test!("whitelist");
}
