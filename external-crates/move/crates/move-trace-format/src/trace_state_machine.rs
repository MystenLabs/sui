// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    trace_format::{
        DataLoad, Effect, Location, Read, TraceEvent, TraceLocalUID, TraceValue, Write,
    },
    trace_interface::{Tracer, Writer},
};
use core::fmt;
use move_core_types::annotated_value::MoveValue;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct TraceState {
    pub loaded_state: BTreeMap<TraceLocalUID, MoveValue>,
    pub operand_stack: Vec<TraceValue>,
    pub call_stack: Vec<(BTreeMap<usize, TraceValue>, bool)>,
}

impl TraceState {
    pub fn new() -> Self {
        Self {
            loaded_state: BTreeMap::new(),
            operand_stack: vec![],
            call_stack: vec![],
        }
    }

    /// Given a reference "location" return the value it points to.
    pub fn dereference(&self, location: &Location) -> MoveValue {
        match location {
            Location::Local(frame_idx, idx) => {
                let frame = &self.call_stack[*frame_idx];
                frame.0.get(idx).unwrap().snapshot().clone()
            }
            Location::Indexed(loc, _offset) => self.dereference(loc),
            Location::Global(id) => self.loaded_state[id].clone(),
        }
    }

    /// Apply an event to the state machine and update the locals state accordingly.
    pub fn apply_event(&mut self, event: &TraceEvent) {
        match event {
            TraceEvent::OpenFrame { frame, .. } => {
                let mut locals = BTreeMap::new();
                for (i, p) in frame.parameters.iter().enumerate() {
                    // NB: we may fail on the open of a main frame but thats fine since we have
                    // it in the parameters so we can just pop it and not unwrap it to fail
                    // silently.
                    self.operand_stack.pop();
                    locals.insert(i, p.clone());
                }
                self.call_stack.push((locals, frame.is_native));
            }
            TraceEvent::CloseFrame { .. } => {
                self.call_stack.pop().unwrap();
            }
            TraceEvent::Effect(ef) => match ef {
                Effect::ExecutionError => (),
                Effect::Push(value) => {
                    self.operand_stack.push(value.clone());
                }
                Effect::Pop(_) => {
                    self.operand_stack.pop().unwrap();
                }
                Effect::Read(Read {
                    location,
                    value_read: _,
                    moved,
                }) => {
                    if *moved {
                        match location {
                            Location::Local(frame_idx, idx) => {
                                let frame = &mut self.call_stack[*frame_idx];
                                frame.0.remove(&idx);
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
                    value_written,
                }) => match location {
                    Location::Local(frame_idx, idx) => {
                        let frame = &mut self.call_stack[*frame_idx];
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
                Effect::DataLoad(data_event) => {
                    match data_event {
                        // We only care about by reference loads
                        DataLoad::ByReference {
                            location, snapshot, ..
                        } => {
                            let Location::Global(id) = location else {
                                unreachable!("Dataload by reference must have a global location");
                            };
                            self.loaded_state.insert(*id, snapshot.clone());
                        }
                        // Nothing to do for by value loads since they will be pushed onto the stack or
                        // stored in a local and then referenced from there.
                        DataLoad::ByValue(..) => (),
                    }
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
                let frame = &mut self.call_stack[*frame_idx];
                frame.0.get_mut(idx).unwrap().value_mut().unwrap()
            }
            Location::Indexed(loc, _offset) => self.get_mut_location(loc),
            Location::Global(id) => self.loaded_state.get_mut(id).unwrap(),
        }
    }
}

impl Tracer for TraceState {
    fn notify(&mut self, event: &TraceEvent, _write: Writer<'_>) {
        match event {
            TraceEvent::Instruction { instruction, .. } => {
                println!("{}", self);
                println!("Instruction: {}", instruction);
            }
            TraceEvent::Effect(ef) => println!("Effect: {}", ef),
            TraceEvent::OpenFrame { frame, .. } => {
                println!("{}", self);
                println!("Frame open: {:?}", frame.function_name);
            }
            TraceEvent::CloseFrame { .. } => {
                println!("Frame close");
            }
            _ => (),
        }
        self.apply_event(event);
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
                    format!("{:#}", v).replace("\n", "\n\t  ")
                )?;
            }
        }

        if !self.operand_stack.is_empty() {
            writeln!(f, "Operand stack:")?;
            for (i, v) in self.operand_stack.iter().enumerate() {
                writeln!(f, "\t{}: {}", i, format!("{:#}", v).replace("\n", "\n\t  "))?;
            }
        }

        if !self.call_stack.is_empty() {
            writeln!(f, "Call stack:")?;
            for (i, (frame, _)) in self.call_stack.iter().enumerate() {
                if !frame.is_empty() {
                    writeln!(f, "\tFrame {}:", i)?;
                    for (j, v) in frame.iter() {
                        writeln!(
                            f,
                            "\t\t{}: {}",
                            j,
                            format!("{:#}", v).replace("\n", "\n\t\t")
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}
