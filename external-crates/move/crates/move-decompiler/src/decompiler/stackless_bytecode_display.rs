// Copyright (c) Verichains, 2023

use std::collections::BTreeMap;
use std::fmt::Write;

use move_stackless_bytecode::{function_target::FunctionTarget, stackless_bytecode::Bytecode};
use super::cfg::{self, StacklessBlockIdentifier, StacklessBlockContent, datastructs::Terminator};

pub struct StacklessBycodeDisplayContext<'a> {
    function_target: &'a FunctionTarget<'a>,
    label_offsets: BTreeMap<
        move_stackless_bytecode::stackless_bytecode::Label,
        move_binary_format::file_format::CodeOffset,
    >,
    indent: usize,
    buf: String,
}

#[allow(dead_code)]
impl<'a> StacklessBycodeDisplayContext<'a> {
    pub fn new(function_target: &'a FunctionTarget) -> StacklessBycodeDisplayContext<'a> {
        StacklessBycodeDisplayContext {
            function_target,
            label_offsets: BTreeMap::new(),
            indent: 0,
            buf: String::new(),
        }
    }

    pub fn result(&self) -> &str {
        &self.buf
    }
}

impl<'a> cfg::datastructs::DecompileDisplayContext<StacklessBlockIdentifier, StacklessBlockContent>
    for StacklessBycodeDisplayContext<'a>
{
    fn display_node(
        &mut self,
        node: &cfg::datastructs::BasicBlock<StacklessBlockIdentifier, StacklessBlockContent>,
    ) {
        let insts = &node.content.code;
        let mut iter = insts.iter().enumerate().peekable();

        while let Some((_, code)) = iter.next() {
            if iter.peek().is_none() {
                // last instruction
                match node.next {
                    Terminator::Normal | Terminator::Ret | Terminator::Abort => {}

                    Terminator::Break { .. } | Terminator::Continue { .. } => {
                        if let Bytecode::Jump(..) = code.bytecode {
                            break;
                        } else {
                            panic!("Error: break/continue block does not end with a jump instruction");
                        }
                    }

                    Terminator::IfElse { .. } => {
                        if let Bytecode::Branch(_, _, _, temp_index) = code.bytecode {
                            self.add_lines(&format!("if ($t{}) {{", temp_index));
                        } else {
                            panic!(
                                "Error: if/else terminator does not end with a conditional branch instruction"
                            );
                        }
                        break;
                    }

                    Terminator::Branch { .. } => {
                        if let Bytecode::Jump(..) = code.bytecode {
                            if !code.removed {
                                // no need to check it here anymore, just display the opcode
                                //panic!("Error: unconditional branching opcode not removed");
                            } else {
                                break;
                            }
                        } else {
                            panic!("Error: branch terminator does not end with a jump instruction");
                        }
                    }

                    Terminator::While { .. } => {
                        if let Bytecode::Branch(_, _, _, temp_index) = code.bytecode {
                            self.add_lines(&format!("while ($t{}) {{", temp_index));
                            break;
                        } else {
                            panic!(
                                "Error: While terminator does not end with a conditional branch instruction"
                            );
                        }
                    }
                }
            }

            if code.removed {
                continue;
            }

            match code.bytecode {
                Bytecode::Label(_, label) => {
                    self.add_lines(
                            &format!("// label {} @{}", label.as_usize(), code.original_offset));
                }

                _ => {
                    self.add_lines(&code.bytecode
                            .display(self.function_target, &self.label_offsets).to_string());
                }
            }
        }
    }

    fn block(&mut self, f: impl FnOnce(&mut Self)) {
        self.indent += 1;
        f(self);
        self.indent -= 1;
    }

    fn add_lines(&mut self, lines: &str) {
        let lines = lines.lines();
        for line in lines {
            write!(self.buf, "{}{}\n", "  ".repeat(self.indent), line).unwrap();
        }
    }
}
