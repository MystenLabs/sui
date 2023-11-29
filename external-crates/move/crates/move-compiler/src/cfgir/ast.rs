// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::WarningFilters,
    expansion::ast::{Attributes, Friend, ModuleIdent},
    hlir::ast::{
        BaseType, Command, Command_, FunctionSignature, Label, SingleType, StructDefinition, Var,
        Visibility,
    },
    parser::ast::{ConstantName, FunctionName, StructName, ENTRY_MODIFIER},
    shared::{ast_debug::*, unique_map::UniqueMap},
};
use move_core_types::runtime_value::MoveValue;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, VecDeque};

// HLIR + Unstructured Control Flow + CFG

//**************************************************************************************************
// Program
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct Program {
    pub modules: UniqueMap<ModuleIdent, ModuleDefinition>,
}

//**************************************************************************************************
// Modules
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct ModuleDefinition {
    pub warning_filter: WarningFilters,
    // package name metadata from compiler arguments, not used for any language rules
    pub package_name: Option<Symbol>,
    pub attributes: Attributes,
    pub is_source_module: bool,
    /// `dependency_order` is the topological order/rank in the dependency graph.
    pub dependency_order: usize,
    pub friends: UniqueMap<ModuleIdent, Friend>,
    pub structs: UniqueMap<StructName, StructDefinition>,
    pub constants: UniqueMap<ConstantName, Constant>,
    pub functions: UniqueMap<FunctionName, Function>,
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Constant {
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub loc: Loc,
    pub signature: BaseType,
    pub value: Option<MoveValue>,
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum FunctionBody_ {
    Native,
    Defined {
        locals: UniqueMap<Var, SingleType>,
        start: Label,
        block_info: BTreeMap<Label, BlockInfo>,
        blocks: BasicBlocks,
    },
}
pub type FunctionBody = Spanned<FunctionBody_>;

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Function {
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub visibility: Visibility,
    pub entry: Option<Loc>,
    pub signature: FunctionSignature,
    pub body: FunctionBody,
}

//**************************************************************************************************
// Blocks
//**************************************************************************************************

pub type BasicBlocks = BTreeMap<Label, BasicBlock>;

pub type BasicBlock = VecDeque<Command>;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum LoopEnd {
    // If the generated loop end block was not used
    Unused,
    // The target of breaks inside the loop
    Target(Label),
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct LoopInfo {
    pub is_loop_stmt: bool,
    pub loop_end: LoopEnd,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum BlockInfo {
    LoopHead(LoopInfo),
    Other,
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl LoopEnd {
    pub fn equals(&self, lbl: Label) -> bool {
        match self {
            LoopEnd::Unused => false,
            LoopEnd::Target(t) => *t == lbl,
        }
    }
}

//**************************************************************************************************
// Label util
//**************************************************************************************************

pub fn remap_labels(
    remapping: &BTreeMap<Label, Label>,
    start: Label,
    blocks: BasicBlocks,
) -> (Label, BasicBlocks) {
    let blocks = blocks
        .into_iter()
        .map(|(lbl, mut block)| {
            remap_labels_block(remapping, &mut block);
            (remapping[&lbl], block)
        })
        .collect();
    (remapping[&start], blocks)
}

fn remap_labels_block(remapping: &BTreeMap<Label, Label>, block: &mut BasicBlock) {
    for cmd in block {
        remap_labels_cmd(remapping, cmd)
    }
}

fn remap_labels_cmd(remapping: &BTreeMap<Label, Label>, sp!(_, cmd_): &mut Command) {
    use Command_::*;
    match cmd_ {
        Break(_) | Continue(_) => panic!("ICE break/continue not translated to jumps"),
        Mutate(_, _) | Assign(_, _) | IgnoreAndPop { .. } | Abort(_) | Return { .. } => (),
        Jump { target, .. } => *target = remapping[target],
        JumpIf {
            if_true, if_false, ..
        } => {
            *if_true = remapping[if_true];
            *if_false = remapping[if_false];
        }
    }
}

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl AstDebug for Program {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Program { modules } = self;

        for (m, mdef) in modules.key_cloned_iter() {
            w.write(&format!("module {}", m));
            w.block(|w| mdef.ast_debug(w));
            w.new_line();
        }
    }
}

impl AstDebug for ModuleDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let ModuleDefinition {
            warning_filter,
            package_name,
            attributes,
            is_source_module,
            dependency_order,
            friends,
            structs,
            constants,
            functions,
        } = self;
        warning_filter.ast_debug(w);
        if let Some(n) = package_name {
            w.writeln(&format!("{}", n))
        }
        attributes.ast_debug(w);
        if *is_source_module {
            w.writeln("library module")
        } else {
            w.writeln("source module")
        }
        w.writeln(&format!("dependency order #{}", dependency_order));
        for (mident, _loc) in friends.key_cloned_iter() {
            w.write(&format!("friend {};", mident));
            w.new_line();
        }
        for sdef in structs.key_cloned_iter() {
            sdef.ast_debug(w);
            w.new_line();
        }
        for cdef in constants.key_cloned_iter() {
            cdef.ast_debug(w);
            w.new_line();
        }
        for fdef in functions.key_cloned_iter() {
            fdef.ast_debug(w);
            w.new_line();
        }
    }
}

impl AstDebug for (ConstantName, &Constant) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            Constant {
                warning_filter,
                index,
                attributes,
                loc: _loc,
                signature,
                value,
            },
        ) = self;
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);
        w.write(&format!("const#{index} {name}:"));
        signature.ast_debug(w);
        w.write(" = ");
        match value {
            None => w.write("_|_ /* unfoldable */"),
            Some(v) => v.ast_debug(w),
        }
        w.write(";");
    }
}

impl AstDebug for MoveValue {
    fn ast_debug(&self, w: &mut AstWriter) {
        use MoveValue as V;
        match self {
            V::U8(u) => w.write(&format!("{}", u)),
            V::U16(u) => w.write(&format!("{}", u)),
            V::U32(u) => w.write(&format!("{}", u)),
            V::U64(u) => w.write(&format!("{}", u)),
            V::U128(u) => w.write(&format!("{}", u)),
            V::U256(u) => w.write(&format!("{}", u)),
            V::Bool(b) => w.write(&format!("{}", b)),
            V::Address(a) => w.write(&format!("{}", a)),
            V::Vector(vs) => {
                w.write("vector[");
                w.comma(vs, |w, v| v.ast_debug(w));
                w.write("]");
            }
            V::Struct(_) => panic!("ICE struct constants not supported"),
            V::Variant(_) => panic!("ICE enum constants not supported"),
            V::Signer(_) => panic!("ICE signer constants not supported"),
        }
    }
}

impl AstDebug for (FunctionName, &Function) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            Function {
                warning_filter,
                index,
                attributes,
                visibility,
                entry,
                signature,
                body,
            },
        ) = self;
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);
        visibility.ast_debug(w);
        if entry.is_some() {
            w.write(&format!("{} ", ENTRY_MODIFIER));
        }
        if let FunctionBody_::Native = &body.value {
            w.write("native ");
        }
        w.write(&format!("fun#{index} {name}"));
        signature.ast_debug(w);
        match &body.value {
            FunctionBody_::Defined {
                locals,
                start,
                block_info,
                blocks,
            } => w.block(|w| {
                w.write("locals:");
                w.indent(4, |w| {
                    w.list(locals, ",", |w, (_, v, st)| {
                        w.write(&format!("{}: ", v));
                        st.ast_debug(w);
                        true
                    })
                });
                w.new_line();
                w.writeln("block info:");
                w.indent(4, |w| {
                    for (lbl, info) in block_info {
                        w.writeln(&format!("{lbl}: "));
                        info.ast_debug(w);
                    }
                });
                w.writeln(&format!("start={}", start.0));
                w.new_line();
                blocks.ast_debug(w);
            }),
            FunctionBody_::Native => w.writeln(";"),
        }
    }
}

impl AstDebug for BasicBlocks {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.list(self, "", |w, lbl_block| {
            lbl_block.ast_debug(w);
            w.new_line();
            true
        })
    }
}

impl AstDebug for (&Label, &BasicBlock) {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(&format!("label {}:", (self.0).0));
        w.indent(4, |w| w.semicolon(self.1, |w, cmd| cmd.ast_debug(w)))
    }
}

impl AstDebug for BlockInfo {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            BlockInfo::LoopHead(i) => i.ast_debug(w),
            BlockInfo::Other => w.write("non-loop head"),
        }
    }
}

impl AstDebug for LoopInfo {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Self {
            is_loop_stmt,
            loop_end,
        } = self;
        w.write(&format!(
            "{{ is_loop_stmt: {}, end: ",
            if *is_loop_stmt { "true" } else { "false" }
        ));
        loop_end.ast_debug(w);
        w.write(" }}")
    }
}

impl AstDebug for LoopEnd {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            LoopEnd::Unused => w.write("unused end"),
            LoopEnd::Target(lbl) => w.write(&format!("{lbl}")),
        }
    }
}
