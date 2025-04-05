// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

use crate::execution::values::{values_impl::Value, MemBox};

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
    values: HashMap<BaseHeapId, MemBox<Value>>,
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
    slice: Vec<MemBox<Value>>,
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
        self.values.insert(next_id, MemBox::new(value));
        let ref_ = self.borrow_loc(next_id)?;
        Ok((next_id, ref_))
    }

    /// Moves a location out of memory
    pub fn take_loc(&mut self, ndx: BaseHeapId) -> PartialVMResult<Value> {
        if self.is_invalid(ndx)? {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Cannot move from an invalid memory location".to_string()),
            );
        }

        let Some(value_box) = self.values.remove(&ndx) else {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Invalid index: {}", ndx)),
            );
        };

        value_box.take()
    }

    /// Borrows the specified location
    pub fn borrow_loc(&self, ndx: BaseHeapId) -> PartialVMResult<Value> {
        self.values
            .get(&ndx)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Local index out of bounds: {}", ndx))
            })
            .map(|value| value.as_ref_value())
    }

    /// Checks if the value at the location is invalid
    pub fn is_invalid(&self, ndx: BaseHeapId) -> PartialVMResult<bool> {
        self.values
            .get(&ndx)
            .map(|value| matches!(&*value.borrow(), &Value::Invalid))
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
            .map(MemBox::new) // Make them into MemBoxes
            .collect::<Vec<MemBox<Value>>>();

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
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, MemBox<Value>> {
        self.slice.iter()
    }

    /// Makes a copy of the value, via `value.copy_value`
    pub fn copy_loc(&self, ndx: usize) -> PartialVMResult<Value> {
        self.get_valid(ndx).map(|value| value.borrow().copy_value())
    }

    /// Moves a location out of memory, swapping it with `ValueImpl::Invalid`
    pub fn move_loc(&mut self, ndx: usize) -> PartialVMResult<Value> {
        let value_slot = self.get_valid_mut(ndx)?;
        Ok(std::mem::replace(
            &mut *value_slot.borrow_mut(),
            Value::invalid(),
        ))
    }

    pub fn borrow_loc(&mut self, ndx: usize) -> PartialVMResult<Value> {
        self.get_valid_mut(ndx).map(|value| value.as_ref_value())
    }

    /// Stores the value at the location
    pub fn store_loc(&mut self, ndx: usize, x: Value) -> PartialVMResult<()> {
        if ndx >= self.slice.len() {
            return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Local index out of bounds: {}", ndx)));
        }
        let _ = self.slice[ndx].replace(x);
        Ok(())
    }

    /// Gets an index, or returns an error if the index is out of range or the value is unset.
    fn get_valid(&self, ndx: usize) -> PartialVMResult<&MemBox<Value>> {
        self.slice
            .get(ndx)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Local index out of bounds: {}", ndx))
            })
            .and_then(|value| {
                if matches!(&*value.borrow(), &Value::Invalid) {
                    Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!("Local index {} is unset", ndx)))
                } else {
                    Ok(value)
                }
            })
    }

    /// Gets an index, or returns an error if the index is out of range or the value is unset.
    fn get_valid_mut(&mut self, ndx: usize) -> PartialVMResult<&mut MemBox<Value>> {
        self.slice
            .get_mut(ndx)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Local index out of bounds: {}", ndx))
            })
            .and_then(|value| {
                if matches!(&*value.borrow(), &Value::Invalid) {
                    Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!("Local index {} is unset", ndx)))
                } else {
                    Ok(value)
                }
            })
    }

    pub fn drop_all_values(&mut self) -> impl Iterator<Item = Value> {
        self.slice
            .iter_mut()
            .filter_map(|value| match &mut *value.borrow_mut() {
                Value::Invalid => None,
                value @ Value::Reference(_) => {
                    *value = Value::Invalid;
                    None
                }
                value @ (Value::U8(_)
                | Value::U16(_)
                | Value::U32(_)
                | Value::U64(_)
                | Value::U128(_)
                | Value::U256(_)
                | Value::Bool(_)
                | Value::Address(_)
                | Value::Vec(_)
                | Value::PrimVec(_)
                | Value::Struct(_)
                | Value::Variant(_)) => {
                    let result = std::mem::replace(value, Value::Invalid);
                    Some(result)
                }
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[cfg(test)]
    #[allow(non_snake_case)]
    /// This is strictly for testing cycle dropping.
    /// If you ever mark this not #[cfg(test)] you will have your VM implementor card revoked.
    pub(crate) fn UNSAFE_copy_local_box(&mut self, ndx: usize) -> MemBox<Value> {
        self.slice[ndx].UNSAFE_ptr_clone()
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
