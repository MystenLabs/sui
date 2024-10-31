// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

use crate::{
    execution::values::{values_impl::Value, ValueImpl},
    shared::constants::{CALL_STACK_SIZE_LIMIT, LOCALS_PER_FRAME_LIMIT}, cache::arena,
};

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
pub struct MachineHeap {
    heap: Box<[Value]>,
    current_index: u64,
}

/// A stack frame is an allocated frame. It was allocated starting at `start` in the heap. When it
/// is freed, we need to check that we are freeing the one on the end of the heap.
#[derive(Debug)]
pub struct StackFrame {
    base_index: usize,
    slice: *mut [Value],
}

// -------------------------------------------------------------------------------------------------
// Base (Machine-External) Heap
// -------------------------------------------------------------------------------------------------

impl BaseHeap {

    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            next_id: 0,
        }
    }

    /// Allocate a slot for the value in the base heap, and then
    pub fn allocate_and_borrow_loc(&mut self, value: Value) -> PartialVMResult<(BaseHeapId, Value)> {
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
                    .with_message(format!("Invalid index: {}", ndx)));
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
                    .with_message(format!("Index out of bounds: {}", ndx))
            })
            .and_then(|value| Ok(value.take_ref()?))
    }

    /// Checks if the value at the location is invalid
    pub fn is_invalid(&self, ndx: BaseHeapId) -> PartialVMResult<bool> {
        self.values
            .get(&ndx)
            .map(|value| matches!(value.as_ref(), &Value(ValueImpl::Invalid)))
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Invalid index: {}", ndx))
            })
    }

}

// -------------------------------------------------------------------------------------------------
// Machine (Runtime) Heap
// -------------------------------------------------------------------------------------------------

impl MachineHeap {
    pub fn new() -> Self {
        let heap = (0..CALL_STACK_SIZE_LIMIT * LOCALS_PER_FRAME_LIMIT).map(|_| Value::invalid()).collect::<Vec<_>>().into_boxed_slice();
        Self {
            heap,
            current_index: 0,
        }
    }

    /// Allocates a stack frame with the given size.
    /// If there is not enough space in the heap, it returns an error.
    pub fn allocate_stack_frame(
        &mut self,
        params: Vec<Value>,
        size: usize,
    ) -> PartialVMResult<StackFrame> {
        let base_index = self.current_index as usize;
        let remaining_space = self.heap.len() - base_index;

        // Check if there's enough space to allocate the frame
        if size > remaining_space {
            return Err(
                PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED).with_message(format!(
                    "Failed to allocate stack frame: requested size {}, remaining space {}",
                    size, remaining_space
                )),
            );
        }

        // Calculate how many invalid values need to be added
        let invalids_len = size - params.len();

        // Initialize the stack frame with the provided parameters and fill remaining slots with `Invalid`
        let local_values = params
            .into_iter()
            .chain((0..invalids_len).map(|_| Value::invalid())) // Fill the rest with `Invalid`
            .collect::<Vec<Value>>();

        // Create the stack frame
        // SAFETY: We are manually creating a slice from the heap array with known bounds,
        // and we ensure that the range does not exceed the heap size.
        let slice = {
            // This is safe because we already checked bounds above.
            let slice = &self.heap[base_index..base_index + size];
            slice as *const [Value] as *mut [Value]
        };
        {
            let borrow_slice = arena::to_mut_ref_slice(slice);
            for (ndx, value) in local_values.into_iter().enumerate() {
                borrow_slice[ndx] = value;
            }
        }
        let frame = StackFrame { base_index, slice };

        // Move the current index forward
        self.current_index += size as u64;

        Ok(frame)
    }

    /// Frees the given stack frame, ensuring that it is the last frame on the heap.
    pub fn free_stack_frame(&mut self, frame: StackFrame) -> PartialVMResult<()> {
        let current_index = self.current_index as usize;

        // Ensure that we are freeing the most recently allocated frame
        if frame.base_index + frame.slice.len() != current_index {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    "Attempt to free a stack frame that is not the last allocated frame"
                        .to_string(),
                ),
            );
        }

        // Move the current index back, effectively freeing the space
        self.current_index -= frame.slice.len() as u64;

        Ok(())
    }
}

// -------------------------------------------------------------------------------------------------
// Stack Frame
// -------------------------------------------------------------------------------------------------

impl StackFrame {
    pub fn iter(&self) -> std::slice::Iter<'_, Value> {
        arena::mut_to_ref_slice(self.slice).iter()
    }

    /// Only used for debug prints
    pub fn base_index(&self) -> usize {
        self.base_index
    }

    /// Makes a copy of the value, via `value.copy_value`
    pub fn copy_loc(&self, ndx: usize) -> PartialVMResult<Value> {
        if self.is_invalid(ndx)? {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Cannot copy from an invalid memory location".to_string()),
            );
        }
        arena::mut_to_ref_slice(self.slice)
            .get(ndx)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Index out of bounds: {}", ndx))
            })
            .and_then(|value| Ok(value.copy_value()))
    }

    /// Moves a location out of memory, swapping it with `ValueImpl::Invalid`
    pub fn move_loc(&mut self, ndx: usize) -> PartialVMResult<Value> {
        if self.is_invalid(ndx)? {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Cannot move from an invalid memory location".to_string()),
            );
        }

        let value = std::mem::replace(&mut arena::to_mut_ref_slice(self.slice)[ndx], Value::invalid());
        Ok(value)
    }

    /// Stores the value at the location
    pub fn store_loc(&mut self, ndx: usize, x: Value) -> PartialVMResult<()> {
        if ndx >= self.slice.len() {
            return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Index out of bounds: {}", ndx)));
        }
        arena::to_mut_ref_slice(self.slice)[ndx] = x;
        Ok(())
    }

    pub fn borrow_loc(&self, ndx: usize) -> PartialVMResult<Value> {
        if self.is_invalid(ndx)? {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Cannot copy from an invalid memory location".to_string()),
            );
        }
        arena::mut_to_ref_slice(self.slice)
            .get(ndx)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Index out of bounds: {}", ndx))
            })
            .and_then(|value| Ok(value.take_ref()?))
    }

    /// Checks if the value at the location is invalid
    pub fn is_invalid(&self, ndx: usize) -> PartialVMResult<bool> {
        arena::mut_to_ref_slice(self.slice)
            .get(ndx)
            .map(|value| matches!(value, Value(ValueImpl::Invalid)))
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("Index out of bounds: {}", ndx))
            })
    }

    /// Drop all Move values onto a different Vec to avoid leaking memory.
    /// References are excluded since they may point to invalid data.
    pub fn drop_all_values(&mut self) -> impl Iterator<Item = (usize, Value)> {
        let mut res = vec![];

        for ndx in 0..self.slice.len() {
            match &arena::mut_to_ref_slice(self.slice)[ndx].0 {
                ValueImpl::Invalid => (),
                ValueImpl::Reference(_) => {
                    arena::to_mut_ref_slice(self.slice)[ndx] = Value(ValueImpl::Invalid);
                }
                _ => res.push((
                    ndx,
                    std::mem::replace(&mut arena::to_mut_ref_slice(self.slice)[ndx], Value::invalid()),
                )),
            }
        }

        res.into_iter()
    }

}

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------

impl std::fmt::Display for MachineHeap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Heap(current_index: {})", self.current_index)?;
        for (i, value) in self.heap.iter().enumerate() {
            writeln!(f, "  [{}]: {:?}", i, value)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for StackFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "StackFrame(start: {}, size: {})",
            self.base_index,
            self.slice.len()
        )?;
        for (i, value) in arena::mut_to_ref_slice(self.slice).iter().enumerate() {
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
        // _ = self.drop_all_values();
    }
}

/*
{
    /// Creates a new stack frame from a set of parameter values and a total size.
    /// - `params` is initial params, **in order** (ordered `0, 1, ..., n`).
    /// - `size` is total size, which should include `params` size.
    pub fn allocate_stack_frame(params: Vec<Value>, size: usize) -> &[] {
        debug_assert!(size < LOCALS_PER_FRAME_LIMIT);
        let invalids_len = size - params.len();
        let local_values = params
            .into_iter()
            .map(|v| v.0)
            .chain((0..invalids_len).map(|_| ValueImpl::Invalid))
            .collect();
        Self(Rc::new(RefCell::new(local_values)))
    }

    pub fn copy_loc(&self, ndx: usize) -> PartialVMResult<Value> {
        let v = self.0.borrow();
        match v.get(ndx) {
            Some(ValueImpl::Invalid) => Err(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            )
            .with_message(format!("cannot copy invalid value at index {}", ndx))),
            Some(v) => Ok(Value(v.copy_value()?)),
            None => Err(
                PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                    format!("local index out of bounds: got {}, len: {}", ndx, v.len()),
                ),
            ),
        }
    }

    #[inline]
    fn swap_loc(&mut self, ndx: usize, x: Value, violation_check: bool) -> PartialVMResult<Value> {
        let mut v = self.0.borrow_mut();
        match v.get_mut(ndx) {
            Some(v) => {
                if violation_check {
                    if let ValueImpl::Container(c) = v {
                        if c.rc_count() > 1 {
                            return Err(PartialVMError::new(
                                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            )
                            .with_message(
                                "moving container with dangling references".to_string(),
                            ));
                        }
                    }
                }
                Ok(Value(std::mem::replace(v, x.0)))
            }
            None => Err(
                PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                    format!("local index out of bounds: got {}, len: {}", ndx, v.len()),
                ),
            ),
        }
    }

    pub fn move_loc(&mut self, ndx: usize, violation_check: bool) -> PartialVMResult<Value> {
        match self.swap_loc(ndx, Value(ValueImpl::Invalid), violation_check)? {
            Value(ValueImpl::Invalid) => Err(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            )
            .with_message(format!("cannot move invalid value at index {}", ndx))),
            v => Ok(v),
        }
    }

    pub fn store_loc(
        &mut self,
        ndx: usize,
        x: Value,
        violation_check: bool,
    ) -> PartialVMResult<()> {
        self.swap_loc(ndx, x, violation_check)?;
        Ok(())
    }

    /// Drop all Move values onto a different Vec to avoid leaking memory.
    /// References are excluded since they may point to invalid data.
    pub fn drop_all_values(&mut self) -> impl Iterator<Item = (usize, Value)> {
        let mut res = vec![];

        for ndx in 0..self.slice.len() {
            match &self.slice[ndx] {
                ValueImpl::Invalid => (),
                ValueImpl::ContainerRef(_) | ValueImpl::IndexedRef(_) => {
                    self.slice[ndx] = ValueImpl::Invalid;
                }
                _ => res.push((
                    ndx,
                    Value(std::mem::replace(&mut self.slice[ndx], ValueImpl::Invalid)),
                )),
            }
        }

        res.into_iter()
    }

    pub fn is_invalid(&self, ndx: usize) -> PartialVMResult<bool> {
        let v = self.0.borrow();
        match v.get(ndx) {
            Some(ValueImpl::Invalid) => Ok(true),
            Some(_) => Ok(false),
            None => Err(
                PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                    format!("local index out of bounds: got {}, len: {}", ndx, v.len()),
                ),
            ),
        }
    }
}
*/
