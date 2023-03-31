// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use move_core_types::account_address::AccountAddress;

use crate::source_package::parsed_manifest::{NamedAddress, PackageName};

/// A named address qualified with the package name whose scope it belongs to.
pub type QualifiedAddress = (PackageName, NamedAddress);

/// A data structure for unifying named addresses across packages according to renamings and
/// assigning them numerical addresses.
#[derive(Debug)]
pub struct ResolvingTable {
    /// Disjoint set data structure for assignments which can either hold no value, a fixed value or
    /// a forwarding reference to another element in the table.  Each entry in the `redirection`
    /// table gets a slot in the `assignment` table.
    assignments: Vec<Assignment>,

    /// Mapping named addresses to an entry in the `assignments` table.
    redirection: BTreeMap<QualifiedAddress, usize>,
}

#[derive(Debug, PartialEq, Eq)]
enum Assignment {
    Assign(Option<AccountAddress>),
    Linked(usize),
}

impl ResolvingTable {
    /// A fresh `ResolvingTable` with no bindings.
    pub fn new() -> ResolvingTable {
        ResolvingTable {
            assignments: Vec::new(),
            redirection: BTreeMap::new(),
        }
    }

    /// Iterates over the bindings in this table that are within `pkg`'s scope.
    pub fn bindings(
        &self,
        pkg: PackageName,
    ) -> impl Iterator<Item = (NamedAddress, &Option<AccountAddress>)> {
        let start = (pkg, NamedAddress::from(""));
        self.redirection
            .range(start..)
            .take_while(move |((scope, _), _)| *scope == pkg)
            .map(|((_, name), ix)| (*name, self.parent(*ix)))
    }

    /// Return a reference to the address that `name` is currently bound to, if one exists, or
    /// `None` otherwise.
    pub fn get(&self, name: QualifiedAddress) -> Option<&AccountAddress> {
        self.parent(*self.redirection.get(&name)?).as_ref()
    }

    /// Indicates whether there is a binding in this resolving table for `name`.  A table contains a
    /// binding if it has previously been passed into a call to `define` or `unify` (even if it has
    /// not been assigned a concrete numerical address).
    pub fn contains(&self, name: QualifiedAddress) -> bool {
        self.redirection.contains_key(&name)
    }

    /// Add the binding `name = addr` to the table and propagate it across all renamings that
    /// transitively involve `name`.  Fails if this introduces a contradiction (A path through
    /// bindings between two account addresses that are unequal to each other), and succeeds
    /// otherwise.
    pub fn define(&mut self, name: QualifiedAddress, addr: Option<AccountAddress>) -> Result<()> {
        let ix = self.get_or_create_assignment(name);
        let Assignment::Assign(slot) = &mut self.assignments[ix] else {
            unreachable!("Non-root assignment");
        };

        match (slot, addr) {
            (_, None) => { /* nop */ }
            (slot @ None, addr) => *slot = addr,
            (Some(existing), Some(new)) => {
                if *existing != new {
                    return Err(unification_error(name.1, *existing, new));
                }
            }
        }

        Ok(())
    }

    /// Add the binding `a = b` to the table.  Fails if this introduces a contradiction (A path
    /// through bindings between two account addresses that are unequal to each other), and succeeds
    /// otherwise.
    pub fn unify(&mut self, a: QualifiedAddress, b: QualifiedAddress) -> Result<()> {
        let ix = self.get_or_create_assignment(a);
        let jx = self.get_or_create_assignment(b);

        if ix == jx {
            return Ok(());
        }

        let Assignment::Assign(ia) = self.assignments[ix] else {
            unreachable!("Non-root assignment");
        };

        let Assignment::Assign(ja) = self.assignments[jx] else {
            unreachable!("Non-root assignment");
        };

        match (ia, ja) {
            (None, Some(_)) => self.assignments[ix] = Assignment::Linked(jx),

            (Some(_), None) | (None, None) => self.assignments[jx] = Assignment::Linked(ix),

            (Some(ia), Some(ja)) => {
                if ia != ja {
                    return Err(unification_error(a.1, ia, ja));
                }
            }
        };

        Ok(())
    }

    /// Returns the index of the "root" assignment (i.e. not a link to another assignment) for
    /// `name` in this table, creating an empty assignment if one does not already exist.
    ///
    /// Performs path compression on the internal links to speed up future look-ups.
    fn get_or_create_assignment(&mut self, name: QualifiedAddress) -> usize {
        let Some(mut c) = self.redirection.get(&name).copied() else {
            self.assignments.push(Assignment::Assign(None));
            self.redirection.insert(name, self.assignments.len() - 1);
            return self.assignments.len() - 1;
        };

        let Assignment::Linked(mut p) = self.assignments[c] else {
            return c;
        };

        let Assignment::Linked(mut gp) = self.assignments[p] else {
            return p;
        };

        let mut chain = vec![c];
        while let Assignment::Linked(ggp) = self.assignments[gp] {
            (c, p, gp) = (p, gp, ggp);
            chain.push(c);
        }

        for link in chain {
            self.assignments[link] = Assignment::Linked(gp);
        }

        gp
    }

    /// Chase links from the assignent at index `ix` until a non-link `Assignment` is found, and
    /// return a reference to that.
    fn parent(&self, mut ix: usize) -> &Option<AccountAddress> {
        loop {
            match &self.assignments[ix] {
                Assignment::Linked(p) => ix = *p,
                Assignment::Assign(addr) => return addr,
            }
        }
    }
}

fn unification_error(name: NamedAddress, a: AccountAddress, b: AccountAddress) -> anyhow::Error {
    anyhow!(
        "Conflicting assignments for address '{name}': '0x{}' and '0x{}'.",
        a.short_str_lossless(),
        b.short_str_lossless(),
    )
}
