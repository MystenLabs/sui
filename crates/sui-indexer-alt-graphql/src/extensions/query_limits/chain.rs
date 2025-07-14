// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::rc::Rc;

use async_graphql::{Name, PathSegment};

/// Chains represent a tree of [PathSegment]s, where each link in the chain knows its parent. They
/// are used to recover the path of (nested) fields at which an error occurred.
pub(super) struct Chain {
    seg: PathSegment,
    pred: Option<Rc<Chain>>,
}

impl Chain {
    /// Create a new chain with `name` appended to `pred`.
    pub(super) fn new(pred: Option<Rc<Chain>>, name: Name) -> Rc<Self> {
        Rc::new(Self {
            seg: PathSegment::Field(name.as_str().to_owned()),
            pred,
        })
    }

    /// Recover the path ending at this chain node.
    pub(super) fn to_path(&self) -> Vec<PathSegment> {
        let mut path = vec![];
        let mut curr = self;
        loop {
            path.push(curr.seg.clone());
            if let Some(pred) = &curr.pred {
                curr = pred;
            } else {
                break;
            }
        }

        path.reverse();
        path
    }

    /// Convenience function for taking a predecessor edge and turning it into a path.
    pub(super) fn path(pred: &Option<Rc<Chain>>) -> Vec<PathSegment> {
        pred.as_ref().map_or(vec![], |p| p.to_path())
    }
}
