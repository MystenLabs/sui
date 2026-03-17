// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A trait for tracking the resource limits consumed while serializing a value.
pub trait Meter {
    type Nested<'a>: Meter
    where
        Self: 'a;

    /// Produce a new meter for nested contexts, consuming one unit of nesting budget.
    fn nest(&mut self) -> Result<Self::Nested<'_>, MeterError>;

    /// Charge `amount` units against the remaining size budget.
    fn charge(&mut self, amount: usize) -> Result<(), MeterError>;
}

#[derive(Clone, Copy, Default)]
pub struct Unmetered;

/// A meter backed by mutable local budget state.
///
/// "Local" means nested visitors share a single remaining size budget reference while each meter
/// handle keeps its own remaining depth budget.
pub struct LocalMeter<'a> {
    size_budget: &'a mut usize,
    depth_budget: usize,
}

#[derive(thiserror::Error, Debug, Clone, Copy)]
pub enum MeterError {
    #[error("Deserialized value too large")]
    TooBig,

    #[error("Exceeded maximum depth")]
    TooDeep,
}

impl<'a> LocalMeter<'a> {
    pub fn new(size_budget: &'a mut usize, depth_budget: usize) -> Self {
        Self {
            size_budget,
            depth_budget,
        }
    }
}

impl Meter for Unmetered {
    type Nested<'a>
        = Self
    where
        Self: 'a;

    fn nest(&mut self) -> Result<Self::Nested<'_>, MeterError> {
        Ok(*self)
    }

    fn charge(&mut self, _amount: usize) -> Result<(), MeterError> {
        Ok(())
    }
}

impl Meter for LocalMeter<'_> {
    type Nested<'a>
        = LocalMeter<'a>
    where
        Self: 'a;

    fn nest(&mut self) -> Result<Self::Nested<'_>, MeterError> {
        if self.depth_budget == 0 {
            Err(MeterError::TooDeep)
        } else {
            Ok(LocalMeter::new(self.size_budget, self.depth_budget - 1))
        }
    }

    fn charge(&mut self, amount: usize) -> Result<(), MeterError> {
        if *self.size_budget < amount {
            Err(MeterError::TooBig)
        } else {
            *self.size_budget -= amount;
            Ok(())
        }
    }
}
