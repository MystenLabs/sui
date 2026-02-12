// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{MeterError, MeterResult};

pub trait Meter {
    type Error;

    fn visit_nodes(&mut self, num_nodes: usize) -> MeterResult<(), Self::Error> {
        self.visit_nodes_impl(num_nodes).map_err(MeterError::Meter)
    }

    fn visit_nodes_impl(&mut self, num_nodes: usize) -> Result<(), Self::Error>;

    fn visit_edges(&mut self, total_edge_size: usize) -> MeterResult<(), Self::Error> {
        self.visit_edges_impl(total_edge_size)
            .map_err(MeterError::Meter)
    }

    fn visit_edges_impl(&mut self, total_edge_size: usize) -> Result<(), Self::Error>;
}

pub struct DummyMeter;

impl Meter for DummyMeter {
    type Error = ();

    fn visit_nodes_impl(&mut self, _num_nodes: usize) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_edges_impl(&mut self, _total_edge_size: usize) -> Result<(), Self::Error> {
        Ok(())
    }
}
