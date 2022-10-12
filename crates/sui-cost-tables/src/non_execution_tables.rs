// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// These tables cover parts of TX processing which do not involve the execution of a TX
// NOTE: all values in this file are subject to change

use move_core_types::gas_algebra::{Byte, InternalGas, InternalGasPerByte, InternalGasUnit};

use crate::units_types::LinearEquation;

// Maximum number of events a call can emit
pub const MAX_NUM_EVENT_EMIT: u64 = 256;

// Maximum gas a TX can use
pub const MAX_TX_GAS: u64 = 1_000_000_000;

//
// Fixed costs: these are charged regardless of execution
//
// This is a flat fee
pub const BASE_TX_COST_FIXED: u64 = 10_000;
// This is charged per byte of the TX
// 0 for now till we decide if we want size dependence
pub const BASE_TX_COST_PER_BYTE: u64 = 0;

//
// Object access costs: These are for reading, writing, and verifying objects
//
// Cost to read an object per byte
pub const OBJ_ACCESS_COST_READ_PER_BYTE: u64 = 15;
// Cost to mutate an object per byte
pub const OBJ_ACCESS_COST_MUTATE_PER_BYTE: u64 = 40;
// Cost to delete an object per byte
pub const OBJ_ACCESS_COST_DELETE_PER_BYTE: u64 = 40;
// For checking locks. Charged per object
pub const OBJ_ACCESS_COST_VERIFY_PER_BYTE: u64 = 200;

//
// Object storage costs: These are for storing objects
//
// Cost to store an object per byte. This is refundable
pub const OBJ_DATA_COST_REFUNDABLE: u64 = 100;
// Cost to store metadata of objects per byte.
// This depends on the size of various fields including the effects
pub const OBJ_METADATA_COST_NON_REFUNDABLE: u64 = 50;

//
// Consensus costs: costs for TXes that use shared object
//
// Flat cost for consensus transactions
pub const CONSENSUS_COST: u64 = 100_000;

//
// Package verification & publish cost: when publishing a package
//
// Flat cost
pub const PACKAGE_PUBLISH_COST_FIXED: u64 = 1_000;
// This is charged per byte of the package
pub const PACKAGE_PUBLISH_COST_PER_BYTE: u64 = 80;

pub const MAXIMUM_TX_GAS: InternalGas = InternalGas::new(MAX_TX_GAS);

pub const PUBLISH_COST_EQUATION: LinearEquation<InternalGasUnit, Byte> = publish_cost_equation();

pub const TRANSACTION_ACCEPTANCE_COST_EQUATION: LinearEquation<InternalGasUnit, Byte> =
    transaction_acceptance_cost_equation();

const fn publish_cost_equation() -> LinearEquation<InternalGasUnit, Byte> {
    let offset = InternalGas::new(PACKAGE_PUBLISH_COST_FIXED);
    let slope = InternalGasPerByte::new(PACKAGE_PUBLISH_COST_PER_BYTE);

    LinearEquation::new(
        slope,
        offset,
        InternalGas::new(PACKAGE_PUBLISH_COST_FIXED),
        InternalGas::new(MAX_TX_GAS),
    )
}

const fn transaction_acceptance_cost_equation() -> LinearEquation<InternalGasUnit, Byte> {
    let offset = InternalGas::new(BASE_TX_COST_FIXED);
    let slope = InternalGasPerByte::new(BASE_TX_COST_PER_BYTE);

    LinearEquation::new(
        slope,
        offset,
        InternalGas::new(PACKAGE_PUBLISH_COST_FIXED),
        InternalGas::new(MAX_TX_GAS),
    )
}
