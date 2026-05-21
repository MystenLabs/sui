// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::shared::{
    constants::{
        HISTORICAL_MAX_TYPE_TO_LAYOUT_NODES, MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX,
        VALUE_DEPTH_MAX,
    },
    safe_ops::SafeArithmetic as _,
};
use move_binary_format::{errors::PartialVMResult, partial_vm_error};
use move_vm_config::runtime::VMConfig;
use std::{collections::HashMap, hash::Hash};

pub mod binary_cache;
pub mod constants;
pub mod gas;
pub mod linkage_context;
pub mod logging;
pub mod safe_ops;
pub mod types;
pub mod views;
pub mod vm_pointer;

#[macro_export]
macro_rules! try_block {
    ($($body:tt)*) => {{
        #[allow(clippy::redundant_closure_call)]
        (|| {
            $($body)*
        })()
    }};
}

// NB: this does the lookup separately from the insertion, as otherwise would require copying the
// key to retrieve the entry and support the error case.
#[allow(clippy::map_entry)]
/// Either returns a BTreeMap of unique keys, or a repeated key if the input keys are not unique.
pub fn unique_map<Key: Hash + Eq, Value>(
    values: impl IntoIterator<Item = (Key, Value)>,
) -> Result<HashMap<Key, Value>, Key> {
    let mut map = HashMap::new();
    for (k, v) in values {
        if map.contains_key(&k) {
            return Err(k);
        } else {
            map.insert(k, v);
        }
    }
    Ok(map)
}

/// Tracks depth and node count during recursive type traversal, enforcing configurable
/// limits on both.
pub struct TypeSize {
    depth: u64,
    node_count: u64,
    max_depth: u64,
    max_nodes: u64,
}

impl TypeSize {
    /// Standard limits for normal type traversal (i.e., not factoring in field types or
    /// "values"/layouts of that type): `TYPE_DEPTH_MAX` depth, `MAX_TYPE_INSTANTIATION_NODES` nodes.
    pub fn for_type_traversal() -> Self {
        Self {
            depth: 0,
            node_count: 0,
            max_depth: TYPE_DEPTH_MAX,
            max_nodes: MAX_TYPE_INSTANTIATION_NODES,
        }
    }

    /// Custom limits for "value"/layout traversal.
    pub fn from_vm_config_for_value_depth(vm_config: &VMConfig) -> Self {
        Self {
            depth: 0,
            node_count: 0,
            max_depth: vm_config
                .runtime_limits_config
                .max_value_nest_depth
                .unwrap_or(VALUE_DEPTH_MAX),
            max_nodes: vm_config
                .max_type_to_layout_nodes
                .unwrap_or(HISTORICAL_MAX_TYPE_TO_LAYOUT_NODES),
        }
    }

    /// Check both depth and node count against limits.
    pub fn check(&self) -> PartialVMResult<()> {
        if self.depth > self.max_depth {
            return Err(partial_vm_error!(VM_MAX_TYPE_DEPTH_REACHED));
        }
        if self.node_count > self.max_nodes {
            return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
        }
        Ok(())
    }

    /// Increment depth for a scoped recursive call, then restore it.
    /// Also increments node count (which is NOT restored — nodes accumulate).
    pub fn enter_type<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> PartialVMResult<T>,
    ) -> PartialVMResult<T> {
        self.depth = self.depth.safe_add(1)?;
        self.node_count = self.node_count.safe_add(1)?;
        self.check()?;
        let result = f(self);
        self.check()?;
        self.depth = self.depth.safe_sub(1)?;
        result
    }

    /// Increment node count without changing depth (for visiting siblings at the same level).
    pub fn incr_node_count(&mut self) -> PartialVMResult<()> {
        self.node_count = self.node_count.safe_add(1)?;
        if self.node_count > self.max_nodes {
            return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
        }
        Ok(())
    }

    pub fn depth(&self) -> u64 {
        self.depth
    }

    pub fn node_count(&self) -> u64 {
        self.node_count
    }

    #[cfg(test)]
    pub fn with_initial_depth(mut self, depth: u64) -> Self {
        self.depth = depth;
        self
    }
}
