// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// These tables cover parts of TX processing which do not involve the execution of a TX
// NOTE: all values in this file are subject to change

use move_core_types::gas_algebra::{Byte, InternalGas, InternalGasPerByte, InternalGasUnit};
use sui_protocol_constants::*;

use crate::units_types::LinearEquation;

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
