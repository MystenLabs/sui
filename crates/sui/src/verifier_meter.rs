// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_bytecode_verifier_meter::{Meter, Scope};
use serde::Serialize;

/// A meter that accumulates all the scopes that it sees, without enforcing a limit.
#[derive(Debug)]
pub(crate) struct AccumulatingMeter {
    pkg_acc: Accumulator,
    mod_acc: Accumulator,
    fun_acc: Accumulator,
}

/// Ticks and child scopes recorded for an individual scope.
#[derive(Clone, Debug, Serialize)]
pub struct Accumulator {
    pub name: String,
    #[serde(skip)]
    pub scope: Scope,
    pub ticks: u128,
    pub children: Vec<Accumulator>,
}

impl AccumulatingMeter {
    pub fn new() -> Self {
        Self {
            pkg_acc: Accumulator::new("<unknown>", Scope::Package),
            mod_acc: Accumulator::new("<unknown>", Scope::Module),
            fun_acc: Accumulator::new("<unknown>", Scope::Function),
        }
    }

    pub fn accumulator(&self, scope: Scope) -> &Accumulator {
        match scope {
            Scope::Transaction => unreachable!("transaction scope is not supported"),
            Scope::Package => &self.pkg_acc,
            Scope::Module => &self.mod_acc,
            Scope::Function => &self.fun_acc,
        }
    }

    pub fn accumulator_mut(&mut self, scope: Scope) -> &mut Accumulator {
        match scope {
            Scope::Transaction => unreachable!("transaction scope is not supported"),
            Scope::Package => &mut self.pkg_acc,
            Scope::Module => &mut self.mod_acc,
            Scope::Function => &mut self.fun_acc,
        }
    }
}

impl Accumulator {
    fn new(name: &str, scope: Scope) -> Self {
        Self {
            name: name.to_string(),
            scope,
            ticks: 0,
            children: vec![],
        }
    }

    /// Find the max ticks spent verifying `scope`s within this scope (including itself).
    pub fn max_ticks(&self, scope: Scope) -> u128 {
        let mut accs = vec![self];

        let mut curr = 0u128;
        while let Some(acc) = accs.pop() {
            if acc.scope == scope {
                curr = curr.max(acc.ticks);
            }

            accs.extend(acc.children.iter());
        }

        curr
    }
}

impl Meter for AccumulatingMeter {
    fn enter_scope(&mut self, name: &str, scope: Scope) {
        *self.accumulator_mut(scope) = Accumulator::new(name, scope);
    }

    fn transfer(&mut self, from: Scope, to: Scope, factor: f32) -> PartialVMResult<()> {
        let from_acc = self.accumulator(from).clone();
        let to_acc = self.accumulator_mut(to);

        to_acc.ticks += (from_acc.ticks as f32 * factor) as u128;
        to_acc.children.push(from_acc);
        Ok(())
    }

    fn add(&mut self, scope: Scope, units: u128) -> PartialVMResult<()> {
        self.accumulator_mut(scope).ticks += units;
        Ok(())
    }
}
