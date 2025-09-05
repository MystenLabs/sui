// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result, bail,
    collections::{Path, Paths},
    error,
    regex::Regex,
};
use std::{
    collections::BTreeMap,
    fmt::{self, Debug},
};

//**************************************************************************************************
// Definitions
//**************************************************************************************************

/// A new type wrapper around Ref_ to not expose the internal variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Ref(Ref_);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum Ref_ {
    /// A canonicalized reference--this lets join operate over the same domain
    Canonical(usize),
    /// A reference specific to this block
    Fresh(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Edge<Loc, Lbl: Ord> {
    abstract_size: usize,
    regexes: BTreeMap<Regex<Lbl>, Loc>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    is_mutable: bool,
    pub(crate) abstract_size: usize,
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl Ref {
    pub(crate) fn fresh(id: usize) -> Self {
        Self(Ref_::Fresh(id))
    }
}

impl<Loc, Lbl: Ord> Edge<Loc, Lbl> {
    pub(crate) fn new() -> Self {
        Self {
            abstract_size: 0,
            regexes: BTreeMap::new(),
        }
    }

    pub(crate) fn abstract_size(&self) -> usize {
        self.abstract_size
    }

    pub(crate) fn regexes(&self) -> impl Iterator<Item = &Regex<Lbl>> {
        self.regexes.keys()
    }
}

impl Node {
    pub(crate) fn new(is_mutable: bool) -> Self {
        Self {
            is_mutable,
            abstract_size: 1,
        }
    }

    pub(crate) fn is_mutable(&self) -> bool {
        self.is_mutable
    }
}

//**************************************************************************************************
// extension
//**************************************************************************************************

impl<Loc, Lbl: Ord> Edge<Loc, Lbl> {
    pub(crate) fn insert(&mut self, loc: Loc, regex: Regex<Lbl>) -> usize {
        if self.regexes.contains_key(&regex) {
            // already present, no change in size
            return 0;
        }

        let regex_size = regex.abstract_size();
        self.regexes.insert(regex, loc);
        self.abstract_size = self.abstract_size.saturating_add(regex_size);
        regex_size
    }
}

//**************************************************************************************************
// query
//**************************************************************************************************

impl<Loc: Copy, Lbl: Ord + Clone> Edge<Loc, Lbl> {
    pub(crate) fn paths(&self) -> Paths<Loc, Lbl> {
        self.regexes
            .iter()
            .map(|(regex, &loc)| {
                let (labels, ends_in_dot_star) = regex.query_api_path();
                Path {
                    loc,
                    labels,
                    ends_in_dot_star,
                }
            })
            .collect()
    }
}

//**************************************************************************************************
// canonicalization
//**************************************************************************************************

impl Ref {
    pub fn refresh(self) -> Result<Self> {
        match self.0 {
            Ref_::Canonical(id) => Ok(Self(Ref_::Fresh(id))),
            Ref_::Fresh(_) => {
                bail!("should never refresh a fresh ref. it should have been canonicalized")
            }
        }
    }

    pub fn canonicalize(self, remapping: &BTreeMap<Ref, usize>) -> Result<Self> {
        match self.0 {
            Ref_::Canonical(_) => bail!("should never canonicalize a cnonical ref"),
            Ref_::Fresh(_) => {
                let Some(id) = remapping.get(&self).copied() else {
                    bail!("missing remapping for ref {:?}", self)
                };
                Ok(Self(Ref_::Canonical(id)))
            }
        }
    }

    pub(crate) fn fresh_id(&self) -> Result<usize> {
        match self.0 {
            Ref_::Fresh(id) => Ok(id),
            Ref_::Canonical(_) => bail!("should never get fresh_id from a canonical ref"),
        }
    }
}

//**************************************************************************************************
// joining
//**************************************************************************************************

impl<Loc: Copy, Lbl: Ord + Clone> Edge<Loc, Lbl> {
    // adds all edges in other to self, where the successor/predecessor is in mask
    pub(crate) fn join(&mut self, other: &Self) -> usize {
        let mut size_increase = 0usize;
        for (regex, loc) in &other.regexes {
            size_increase = size_increase.saturating_add(self.insert(*loc, regex.clone()));
        }

        size_increase
    }
}

//**************************************************************************************************
// invariants
//**************************************************************************************************

impl Ref {
    pub fn is_canonical(&self) -> bool {
        matches!(self.0, Ref_::Canonical(_))
    }

    pub fn is_fresh(&self) -> bool {
        matches!(self.0, Ref_::Fresh(_))
    }
}

impl<Loc, Lbl: Ord> Edge<Loc, Lbl> {
    pub(crate) fn check_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            let mut calculated_size = 0;
            for regex in self.regexes.keys() {
                calculated_size += regex.abstract_size();
            }
            debug_assert_eq!(calculated_size, self.abstract_size);
            debug_assert!(!self.regexes.is_empty());
        }
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Ref_::Canonical(id) => write!(f, "l#{}", id),
            Ref_::Fresh(id) => write!(f, "{}", id),
        }
    }
}
