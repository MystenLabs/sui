// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

use crate::execution::values::values_impl::Value;

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;

use std::collections::HashMap;

// -------------------------------------------------------------------------------------------------
// Heap
// -------------------------------------------------------------------------------------------------

/// The Move VM's base heap. This is PTBs and arguments to invocation functions are stored, so that
/// we can handle references to/from them.
#[derive(Debug)]
pub struct BaseHeap {
    next_id: usize,
    values: HashMap<BaseHeapId, Box<Value>>,
}

/// An ID for an entry in a Base Heap.
#[derive(Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct BaseHeapId(usize);

/// The runtime machine "heap" for execution. This allows us to grab and return frame slots and the
/// like. Note that this isn't a _true_ heap (crrently), it only allows for allocating and freeing
/// stackframes.
#[derive(Debug)]
pub struct MachineHeap {}

/// A stack frame is an allocated frame. It was allocated starting at `start` in the heap. When it
/// is freed, we need to check that we are freeing the one on the end of the heap.
#[derive(Debug)]
pub struct StackFrame {
    slice: Vec<Value>,
}

// -------------------------------------------------------------------------------------------------
// Base (Machine-External) Heap
// -------------------------------------------------------------------------------------------------

impl Default for BaseHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseHeap {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            next_id: 0,
        }
    }

    /// Allocate a slot for the value in the base heap, and then
    pub fn allocate_and_borrow_loc(
        &mut self,
        value: Value,
    ) -> PartialVMResult<(BaseHeapId, Value)> {
        let next_id = BaseHeapId(self.next_id);
        self.next_id += 1;
        self.values.insert(next_id, Box::new(value));
        let ref_ = self.borrow_loc(next_id)?;
        Ok((next_id, ref_))
    }

    /// Moves a location out of memory, swapping it with `ValueImpl::Invalid`
    pub fn take_loc(&mut self, ndx: BaseHeapId) -> PartialVMResult<Value> {
        if self.is_invalid(ndx)? {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Cannot move from an invalid memory location".to_string()),
            );
        }

        let Some(value_box) = self.values.get_mut(&ndx) else {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Invalid index: {}", ndx)),
            );
        };

        let value = std::mem::replace(value_box.as_mut(), Value::invalid());
        Ok(value)
    }

    /// Borrows the specified location
    pub fn borrow_loc(&self, ndx: BaseHeapId) -> PartialVMResult<Value> {
        self.values
            .get(&ndx)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Local index out of bounds: {}", ndx))
            })
            .and_then(|value| value.take_ref())
    }

    /// Checks if the value at the location is invalid
    pub fn is_invalid(&self, ndx: BaseHeapId) -> PartialVMResult<bool> {
        self.values
            .get(&ndx)
            .map(|value| matches!(value.as_ref(), &Value::Invalid))
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Invalid index: {}", ndx))
            })
    }
}

// -------------------------------------------------------------------------------------------------
// Machine (Runtime) Heap
// -------------------------------------------------------------------------------------------------

impl Default for MachineHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl MachineHeap {
    pub fn new() -> Self {
        Self {}
    }

    /// Allocates a stack frame with the given size.
    /// If there is not enough space in the heap, it returns an error.
    pub fn allocate_stack_frame(
        &mut self,
        params: Vec<Value>,
        size: usize,
    ) -> PartialVMResult<StackFrame> {
        // Calculate how many invalid values need to be added
        let invalids_len = size - params.len();

        // Initialize the stack frame with the provided parameters and fill remaining slots with `Invalid`
        let local_values = params
            .into_iter()
            .chain((0..invalids_len).map(|_| Value::invalid())) // Fill the rest with `Invalid`
            .collect::<Vec<Value>>();

        Ok(StackFrame {
            slice: local_values,
        })
    }

    /// Frees the given stack frame, ensuring that it is the last frame on the heap.
    pub fn free_stack_frame(&mut self, _frame: StackFrame) -> PartialVMResult<()> {
        Ok(())
    }
}

// -------------------------------------------------------------------------------------------------
// Stack Frame
// -------------------------------------------------------------------------------------------------

impl StackFrame {
    pub fn iter(&self) -> std::slice::Iter<'_, Value> {
        self.slice.iter()
    }

    /// Makes a copy of the value, via `value.copy_value`
    pub fn copy_loc(&self, ndx: usize) -> PartialVMResult<Value> {
        if self.is_invalid(ndx)? {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Cannot copy from an invalid memory location".to_string()),
            );
        }
        self.slice
            .get(ndx)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Local index out of bounds: {}", ndx))
            })
            .map(|value| value.copy_value())
    }

    /// Moves a location out of memory, swapping it with `ValueImpl::Invalid`
    pub fn move_loc(&mut self, ndx: usize) -> PartialVMResult<Value> {
        if self.is_invalid(ndx)? {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Cannot move from an invalid memory location".to_string()),
            );
        }

        let value = std::mem::replace(&mut self.slice[ndx], Value::invalid());
        Ok(value)
    }

    /// Stores the value at the location
    pub fn store_loc(&mut self, ndx: usize, x: Value) -> PartialVMResult<()> {
        if ndx >= self.slice.len() {
            return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Local index out of bounds: {}", ndx)));
        }
        self.slice[ndx] = x;
        Ok(())
    }

    pub fn borrow_loc(&self, ndx: usize) -> PartialVMResult<Value> {
        if self.is_invalid(ndx)? {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Cannot copy from an invalid memory location".to_string()),
            );
        }
        self.slice
            .get(ndx)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Index out of bounds: {}", ndx))
            })
            .and_then(|value| value.take_ref())
    }

    /// Checks if the value at the location is invalid
    pub fn is_invalid(&self, ndx: usize) -> PartialVMResult<bool> {
        self.slice
            .get(ndx)
            .map(|value| matches!(value, Value::Invalid))
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Local index out of bounds: {}", ndx))
            })
    }

    /// Drop all Move values onto a different Vec to avoid leaking memory.
    /// References are excluded since they may point to invalid data.
    pub fn drop_all_values(&mut self) -> impl Iterator<Item = (usize, Value)> {
        let mut res = vec![];

        for (ndx, value) in self.slice.iter_mut().enumerate() {
            match &value {
                Value::Invalid => (),
                Value::Reference(_) => {
                    let _ = std::mem::replace(value, Value::invalid());
                }
                Value::U8(_)
                | Value::U16(_)
                | Value::U32(_)
                | Value::U64(_)
                | Value::U128(_)
                | Value::U256(_)
                | Value::Bool(_)
                | Value::Address(_)
                | Value::Container(_) => {
                    res.push((ndx, std::mem::replace(value, Value::invalid())))
                }
            }
        }
        res.into_iter()
    }
}

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------

impl std::fmt::Display for StackFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "StackFrame(size: {})", self.slice.len())?;
        for (i, value) in self.slice.iter().enumerate() {
            writeln!(f, "  [{}]: {:?}", i, value)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for BaseHeapId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "base#{}", self.0)
    }
}

// -------------------------------------------------------------------------------------------------
// Destructors / Drop
// -------------------------------------------------------------------------------------------------
// Locals may contain reference values that points to the same cotnainer through Rc, hencing forming
// a cycle. Therefore values need to be manually taken out of the Locals in order to not leak memory.

impl Drop for StackFrame {
    fn drop(&mut self) {
        _ = self.drop_all_values();
    }
}
