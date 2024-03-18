// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    interpreter::Interpreter,
    loader::{Function, Loader},
};
use move_binary_format::file_format::Bytecode;
use move_vm_types::values::{self, Locals};
use std::{
    collections::BTreeSet,
    io::{self, Write},
    str::FromStr,
};

#[derive(Debug)]
enum DebugCommand {
    PrintStack,
    Step,
    StepSC,
    Continue,
    ContinueSC,
    Breakpoint(String),
    BreakpointSC(String),
    DeleteBreakpoint(String),
    PrintBreakpoints,
    Help,
}

impl DebugCommand {
    pub fn debug_string(&self) -> &str {
        match self {
            Self::PrintStack => "stack",
            Self::StepSC => "s",
            Self::Step => "step",
            Self::ContinueSC => "c",
            Self::Continue => "continue",
            Self::BreakpointSC(_) => "b ",
            Self::Breakpoint(_) => "breakpoint ",
            Self::DeleteBreakpoint(_) => "delete ",
            Self::PrintBreakpoints => "breakpoints",
            Self::Help => "help",
        }
    }

    pub fn commands() -> Vec<DebugCommand> {
        vec![
            Self::PrintStack,
            Self::Step,
            Self::Continue,
            Self::Breakpoint("".to_string()),
            Self::DeleteBreakpoint("".to_string()),
            Self::PrintBreakpoints,
            Self::Help,
        ]
    }
}

impl FromStr for DebugCommand {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use DebugCommand::*;
        let s = s.trim();
        if s.starts_with(PrintStack.debug_string()) {
            return Ok(PrintStack);
        }
        if s.starts_with(Step.debug_string()) || s.eq(StepSC.debug_string()) {
            return Ok(Step);
        }
        if s.starts_with(Continue.debug_string()) || s.eq(ContinueSC.debug_string()) {
            return Ok(Continue);
        }
        if let Some(breakpoint) = s.strip_prefix(Breakpoint("".to_owned()).debug_string()) {
            return Ok(Breakpoint(breakpoint.to_owned()));
        }

        if let Some(breakpoint) = s.strip_prefix(BreakpointSC("".to_owned()).debug_string()) {
            return Ok(Breakpoint(breakpoint.to_owned()));
        }
        if let Some(breakpoint) = s.strip_prefix(DeleteBreakpoint("".to_owned()).debug_string()) {
            return Ok(DeleteBreakpoint(breakpoint.to_owned()));
        }
        if s.starts_with(PrintBreakpoints.debug_string()) {
            return Ok(PrintBreakpoints);
        }
        if s.starts_with(Help.debug_string()) {
            return Ok(Help);
        }
        Err(format!(
            "Unrecognized command: {}\nAvailable commands: {}",
            s,
            Self::commands()
                .iter()
                .map(|command| command.debug_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

#[derive(Debug)]
pub(crate) struct DebugContext {
    breakpoints: BTreeSet<String>,
    should_take_input: bool,
}

impl DebugContext {
    pub(crate) fn new() -> Self {
        Self {
            breakpoints: BTreeSet::new(),
            should_take_input: true,
        }
    }

    fn delete_breakpoint_at_index(&mut self, breakpoint: &str) {
        let index = breakpoint
            .strip_prefix("at_index")
            .unwrap()
            .trim()
            .parse::<usize>()
            .unwrap();
        self.breakpoints = self
            .breakpoints
            .iter()
            .enumerate()
            .filter_map(|(i, bp)| if i != index { Some(bp.clone()) } else { None })
            .collect();
    }

    fn print_stack(
        function_desc: &Function,
        locals: &Locals,
        pc: u16,
        interp: &Interpreter,
        resolver: &Loader,
    ) {
        let function_string = function_desc.pretty_short_string();
        let mut s = String::new();
        interp.debug_print_stack_trace(&mut s, resolver).unwrap();
        println!("{}", s);
        println!("Current frame: {}\n", function_string);
        let code = function_desc.code();
        println!("        Code:");
        for (i, instr) in code.iter().enumerate() {
            if i as u16 == pc {
                println!("          > [{}] {:?}", pc, instr);
            } else {
                println!("            [{}] {:?}", i, instr);
            }
        }
        println!("        Locals:");
        if function_desc.local_count() > 0 {
            let mut s = String::new();
            values::debug::print_locals(&mut s, locals).unwrap();
            println!("{}", s);
        } else {
            println!("            (none)");
        }
    }

    pub(crate) fn debug_loop(
        &mut self,
        function_desc: &Function,
        locals: &Locals,
        pc: u16,
        instr: &Bytecode,
        resolver: &Loader,
        interp: &Interpreter,
    ) {
        let instr_string = format!("{:?}", instr);
        let function_string = function_desc.pretty_short_string();
        let breakpoint_hit = self
            .breakpoints
            .get(&function_string)
            .or_else(|| {
                self.breakpoints
                    .iter()
                    .find(|bp| instr_string[..].starts_with(bp.as_str()))
            })
            .or_else(|| self.breakpoints.get(&pc.to_string()));

        if self.should_take_input || breakpoint_hit.is_some() {
            self.should_take_input = true;
            if let Some(bp_match) = breakpoint_hit {
                println!(
                    "Breakpoint {} hit with instruction {}",
                    bp_match, instr_string
                );
            }
            println!(
                "function >> {}\ninstruction >> {:?}\nprogram counter >> {}",
                function_string, instr, pc
            );
            Self::print_stack(function_desc, locals, pc, interp, resolver);
            loop {
                print!("> ");
                std::io::stdout().flush().unwrap();
                let mut input = String::new();
                match io::stdin().read_line(&mut input) {
                    Ok(_) => match input.parse::<DebugCommand>() {
                        Err(err) => println!("{}", err),
                        Ok(command) => match command {
                            DebugCommand::Step | DebugCommand::StepSC => {
                                self.should_take_input = true;
                                break;
                            }
                            DebugCommand::Continue | DebugCommand::ContinueSC => {
                                self.should_take_input = false;
                                break;
                            }
                            DebugCommand::Breakpoint(breakpoint)
                            | DebugCommand::BreakpointSC(breakpoint) => {
                                self.breakpoints.insert(breakpoint.to_string());
                            }
                            DebugCommand::DeleteBreakpoint(breakpoint) => {
                                if breakpoint.starts_with("at_index") {
                                    self.delete_breakpoint_at_index(&breakpoint);
                                } else {
                                    self.breakpoints.remove(&breakpoint);
                                }
                            }
                            DebugCommand::PrintBreakpoints => self
                                .breakpoints
                                .iter()
                                .enumerate()
                                .for_each(|(i, bp)| println!("[{}] {}", i, bp)),
                            DebugCommand::PrintStack => {
                                Self::print_stack(function_desc, locals, pc, interp, resolver);
                            }
                            DebugCommand::Help => {
                                println!(
                                    "Available commands:\n\
                                    \tstack: print the current state of the call and value stacks\n\
                                    \tstep (s): step forward one instruction\n\
                                    \tcontinue (c): continue execution until the next breakpoint\n\
                                    \tbreakpoint (b) <string>:\n\
                                    \t\t1. set a breakpoint at the given bytecode instruction that starts with <string>, e.g., Call or CallGeneric\n\
                                    \t\t2. set a breakpoint at the function that matches <string>, e.g., 0x2::vector::pop_back\n\
                                    \t\t3. set a breakpoint at the given code offset, e.g., 10 will stop execution if a code offset of 10 is encountered\n\
                                    \tbreakpoints: print all set breakpoints\n\
                                    \tdelete at_index <int>: delete the breakpoint at index <int> in the set breakpoints\n\
                                    \tdelete <string>: delete the breakpoint matching <string>\n\
                                    \thelp: print this help message"
                                );
                            }
                        },
                    },
                    Err(err) => {
                        println!("Error reading input: {}", err);
                        break;
                    }
                }
            }
        }
    }
}
