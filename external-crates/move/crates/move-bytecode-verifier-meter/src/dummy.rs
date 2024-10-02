// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{Meter, Scope};
use move_binary_format::errors::PartialVMResult;

/// Meter that does nothing.
pub struct DummyMeter;

impl Meter for DummyMeter {
    fn enter_scope(&mut self, _name: &str, _scope: Scope) {}
    fn transfer(&mut self, _from: Scope, _to: Scope, _factor: f32) -> PartialVMResult<()> {
        Ok(())
    }
    fn add(&mut self, _scope: Scope, _units: u128) -> PartialVMResult<()> {
        Ok(())
    }
}
