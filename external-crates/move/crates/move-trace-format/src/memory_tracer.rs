// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains the implementation of the memory tracer. The memory tracer is a tracer
//! that takes a stream of trace events, and uses these events to create a snapshot of the memory
//! state (operand stack, locals, and globals) at each point in time during execution.
//!
//! The memory tracer then emits `External` events with the current VM state for every instruction,
//! and open/close frame event that is has built up.
//!
//! The memory tracer is useful for debugging, and  as an example of how to build up this
//! state for more advanced analysis and also using the custom tracing trait.

use crate::{
    format::{DataLoad, Effect, Location, Read, TraceEvent, TraceIndex, TraceValue, Write},
    interface::{Tracer, Writer},
};
use core::fmt;
use move_core_types::annotated_value::MoveValue;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct TraceState {
    // Tracks "global memory" state (i.e., references out in to global memory/references returned
    // from native functions).
    pub loaded_state: BTreeMap<TraceIndex, MoveValue>,
    // The current state (i.e., values) of the VM's operand stack.
    pub operand_stack: Vec<TraceValue>,
    // The current call stack indexed by frame id. Maps from the frame id to the current state of
    // the frame's locals. The bool indicates if the frame is native or not.
    pub call_stack: BTreeMap<TraceIndex, (BTreeMap<usize, TraceValue>, bool)>,
}

impl TraceState {
    pub fn new() -> Self {
        Self {
            loaded_state: BTreeMap::new(),
            operand_stack: vec![],
            call_stack: BTreeMap::new(),
        }
    }

    /// Apply an event to the state machine and update the locals state accordingly.
    fn apply_event(&mut self, event: &TraceEvent) {
        match event {
            TraceEvent::OpenFrame { frame, .. } => {
                let mut locals = BTreeMap::new();
                for (i, p) in frame.parameters.iter().enumerate() {
                    // NB: parameters are passed directly, so we just pop to make sure they aren't also
                    // left on the operand stack. For the initial call, these pops may (should) fail, but that
                    // is fine as we already have the values in the parameter list.
                    self.operand_stack.pop();
                    locals.insert(i, p.clone());
                }

                self.call_stack
                    .insert(frame.frame_id, (locals, frame.is_native));
            }
            TraceEvent::CloseFrame { .. } => {
                self.call_stack
                    .pop_last()
                    .expect("Unbalanced call stack in memory tracer -- this should never happen");
            }
            TraceEvent::Effect(ef) => match &**ef {
                Effect::ExecutionError(_) => (),
                Effect::Push(value) => {
                    self.operand_stack.push(value.clone());
                }
                Effect::Pop(_) => {
                    self.operand_stack.pop().expect(
                        "Tried to pop off the empty operand stack -- this should never happen",
                    );
                }
                Effect::Read(Read {
                    location,
                    root_value_read: _,
                    moved,
                }) => {
                    if *moved {
                        match location {
                            Location::Local(frame_idx, idx) => {
                                let frame = self.call_stack.get_mut(frame_idx).unwrap();
                                frame.0.remove(idx);
                            }
                            Location::Indexed(..) => {
                                panic!("Cannot move from indexed location");
                            }
                            Location::Global(..) => {
                                panic!("Cannot move from global location");
                            }
                        }
                    }
                }
                Effect::Write(Write {
                    location,
                    root_value_after_write: value_written,
                }) => match location {
                    Location::Local(frame_idx, idx) => {
                        let frame = self.call_stack.get_mut(frame_idx).unwrap();
                        frame.0.insert(*idx, value_written.clone());
                    }
                    Location::Indexed(location, _idx) => {
                        let val = self.get_mut_location(location);
                        *val = value_written.clone().snapshot().clone();
                    }
                    Location::Global(id) => {
                        let val = self.loaded_state.get_mut(id).unwrap();
                        *val = value_written.snapshot().clone();
                    }
                },
                Effect::DataLoad(DataLoad {
                    location, snapshot, ..
                }) => {
                    let Location::Global(id) = location else {
                        unreachable!("Dataload by reference must have a global location");
                    };
                    self.loaded_state.insert(*id, snapshot.clone());
                }
            },
            // External events are treated opaqeuly
            TraceEvent::External(_) => (),
            // Instructions
            TraceEvent::Instruction { .. } => (),
        }
    }

    /// Given a reference "location" return a mutable reference to the value it points to so that
    /// it can be updated.
    fn get_mut_location(&mut self, location: &Location) -> &mut MoveValue {
        match location {
            Location::Local(frame_idx, idx) => {
                let frame = self.call_stack.get_mut(frame_idx).unwrap();
                frame.0.get_mut(idx).unwrap().value_mut().unwrap()
            }
            Location::Indexed(loc, _offset) => self.get_mut_location(loc),
            Location::Global(id) => self.loaded_state.get_mut(id).unwrap(),
        }
    }
}

impl Default for TraceState {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracer for TraceState {
    fn notify(&mut self, event: &TraceEvent, mut write: Writer<'_>) {
        self.apply_event(event);
        // We only emit the state when we hit a non-effect internal event. This coincides with
        // emitting the current state of the VM before each instruction/function call.
        match event {
            TraceEvent::Instruction { .. }
            | TraceEvent::OpenFrame { .. }
            | TraceEvent::CloseFrame { .. } => {
                write.push(self.to_string());
            }
            _ => (),
        }
    }
}

impl fmt::Display for TraceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.loaded_state.is_empty() {
            writeln!(f, "Loaded state:")?;
            for (id, v) in &self.loaded_state {
                writeln!(
                    f,
                    "\t{}: {}",
                    id,
                    format!("{:#}", v).replace('\n', "\n\t  ")
                )?;
            }
        }

        if !self.operand_stack.is_empty() {
            writeln!(f, "Operand stack:")?;
            for (i, v) in self.operand_stack.iter().enumerate() {
                writeln!(f, "\t{}: {}", i, format!("{:#}", v).replace('\n', "\n\t  "))?;
            }
        }

        if !self.call_stack.is_empty() {
            writeln!(f, "Call stack:")?;
            for (i, (frame, _)) in self.call_stack.iter() {
                if !frame.is_empty() {
                    writeln!(f, "\tFrame {}:", i)?;
                    for (j, v) in frame.iter() {
                        writeln!(
                            f,
                            "\t\t{}: {}",
                            j,
                            format!("{:#}", v).replace('\n', "\n\t\t")
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}
