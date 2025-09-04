// Copyright (c) Verichains, 2023

use std::{collections::HashSet, fmt::Display, hash::Hash};

use super::metadata::WithMetadata;

pub trait BlockIdentifierTrait: PartialEq + Eq + Copy + Hash {}
pub trait BlockContentTrait: Clone + Default {}

pub trait DecompileDisplayContext<
    BlockIdentifier: BlockIdentifierTrait,
    BlockContent: BlockContentTrait,
>
{
    fn display_node(&mut self, block: &BasicBlock<BlockIdentifier, BlockContent>);
    fn block(&mut self, f: impl FnOnce(&mut Self));
    fn add_lines(&mut self, lines: &str);
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum BranchType {
    #[default]
    Unknown,
    Continue,
    Break,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct BranchTarget<BlockIdentifier: BlockIdentifierTrait> {
    pub(crate) target: BlockIdentifier,
    pub(crate) branch_type: BranchType,
}
impl<BlockIdentifier: BlockIdentifierTrait> BranchTarget<BlockIdentifier> {
    pub(crate) fn unknown(e: BlockIdentifier) -> BranchTarget<BlockIdentifier> {
        BranchTarget {
            target: e,
            branch_type: BranchType::Unknown,
        }
    }

    pub(crate) fn with_target(&self, target: BlockIdentifier) -> BranchTarget<BlockIdentifier> {
        BranchTarget {
            target,
            branch_type: self.branch_type.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum Terminator<BlockIdentifier: BlockIdentifierTrait> {
    // no terminator
    #[default]
    Normal,
    // has return
    Ret,
    Abort,
    // branches
    IfElse {
        if_block: BranchTarget<BlockIdentifier>,
        else_block: BranchTarget<BlockIdentifier>,
    },
    Branch {
        target: BlockIdentifier,
    },
    While {
        inner_block: BlockIdentifier,
        outer_block: BlockIdentifier,
        content_blocks: Vec<BlockIdentifier>,
    },
    Break {
        target: BlockIdentifier,
    },
    Continue {
        target: BlockIdentifier,
    },
}

#[derive(Default, Debug, Clone)]
pub struct BasicBlock<Identifier: BlockIdentifierTrait, Content: BlockContentTrait> {
    // block index
    pub(crate) idx: usize,

    pub(crate) content: Content,
    // next points to the next blocks
    // conditional branching will be (true, false)
    // unconditional jump will be same for both value
    pub(crate) next: Terminator<Identifier>,

    pub(crate) short_circuit_terminator: Option<(Content, Terminator<Identifier>)>,

    pub(crate) unconditional_loop_entry:
        Option<(usize /* exit */, HashSet<usize> /* contents */)>,
    pub(crate) implicit_terminator: bool,

    pub(crate) has_assignment_variables: HashSet<usize>,
    pub(crate) has_read_variables: HashSet<usize>,

    pub(crate) is_dummy_dispatch_block: bool,
}

#[derive(Debug, Clone)]
pub enum HyperBlock<BlockIdentifier: BlockIdentifierTrait, BlockContent: BlockContentTrait> {
    ConnectedBlocks(Vec<WithMetadata<BasicBlock<BlockIdentifier, BlockContent>>>),
    IfElseBlocks {
        if_unit: Box<WithMetadata<CodeUnitBlock<BlockIdentifier, BlockContent>>>,
        else_unit: Box<WithMetadata<CodeUnitBlock<BlockIdentifier, BlockContent>>>,
    },
    WhileBlocks {
        inner: Box<WithMetadata<CodeUnitBlock<BlockIdentifier, BlockContent>>>,
        outer: Box<WithMetadata<CodeUnitBlock<BlockIdentifier, BlockContent>>>,
        unconditional: bool,
        start_block: usize,
        exit_block: usize,
    },
}

#[derive(Debug, Clone)]
pub struct CodeUnitBlock<BlockIdentifier: BlockIdentifierTrait, BlockContent: BlockContentTrait> {
    pub(crate) blocks: Vec<WithMetadata<HyperBlock<BlockIdentifier, BlockContent>>>,
    pub(crate) terminate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JumpType {
    Unknown,
    If,
    While,
    Continue,
    Break,
}

impl<BlockIdentifier: BlockIdentifierTrait> Terminator<BlockIdentifier> {
    pub fn next_blocks(&self) -> Vec<&BlockIdentifier> {
        match self {
            Terminator::Normal => Vec::new(),
            Terminator::Ret => Vec::new(),
            Terminator::Abort => Vec::new(),
            Terminator::IfElse {
                if_block,
                else_block,
            } => vec![&if_block.target, &else_block.target],
            Terminator::Branch { target } => vec![target],
            Terminator::While {
                inner_block,
                outer_block,
                ..
            } => vec![inner_block, outer_block],
            Terminator::Break { target } => vec![target],
            Terminator::Continue { target } => vec![target],
        }
    }

    pub(crate) fn is_terminated(&self) -> bool {
        matches!(self, Terminator::Ret | Terminator::Abort)
    }

    pub(crate) fn is_terminated_in_loop(&self) -> bool {
        matches!(
            self,
            Terminator::Ret
                | Terminator::Abort
                | Terminator::Break { .. }
                | Terminator::Continue { .. }
        )
    }
}

impl<BlockIdentifier: BlockIdentifierTrait> Display for Terminator<BlockIdentifier> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Terminator::Normal => write!(fmt, "Normal"),
            Terminator::Ret => write!(fmt, "Ret"),
            Terminator::Abort => write!(fmt, "Abort"),
            Terminator::IfElse { .. } => write!(fmt, "IfElse"),
            Terminator::Branch { .. } => write!(fmt, "Branch"),
            Terminator::While { .. } => write!(fmt, "While"),
            Terminator::Break { .. } => write!(fmt, "Break"),
            Terminator::Continue { .. } => write!(fmt, "Continue"),
        }
    }
}

impl<BlockIdentifier: BlockIdentifierTrait, BlockContent: BlockContentTrait> PartialEq
    for BasicBlock<BlockIdentifier, BlockContent>
{
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}
impl<BlockIdentifier: BlockIdentifierTrait, BlockContent: BlockContentTrait> Eq
    for BasicBlock<BlockIdentifier, BlockContent>
{
}

impl<BlockIdentifier: BlockIdentifierTrait, BlockContent: BlockContentTrait>
    BasicBlock<BlockIdentifier, BlockContent>
{
    pub fn new(idx: usize) -> Self {
        BasicBlock {
            idx,
            content: Default::default(),
            next: Terminator::Normal,
            unconditional_loop_entry: None,
            implicit_terminator: false,
            short_circuit_terminator: None,
            has_assignment_variables: Default::default(),
            has_read_variables: Default::default(),
            is_dummy_dispatch_block: false,
        }
    }

    #[allow(dead_code)]
    pub fn referenced_variables_iter(&self) -> impl Iterator<Item = &usize> {
        self.has_assignment_variables
            .iter()
            .chain(self.has_read_variables.iter())
    }

    #[allow(dead_code)]
    fn display<Ctx: DecompileDisplayContext<BlockIdentifier, BlockContent>>(&self, ctx: &mut Ctx) {
        // ctx.add_lines(format!("// block {}, actual {}", self.idx, self.offset).as_str());
        ctx.display_node(&self);
        match self.next {
            Terminator::Ret => {
                ctx.add_lines("// returned path");
            }

            Terminator::Abort => {
                ctx.add_lines("// aborted path");
            }

            Terminator::Break { .. } => {
                ctx.add_lines("break;");
            }

            Terminator::Continue { .. } => {
                if !self.implicit_terminator {
                    ctx.add_lines("continue;");
                }
            }

            Terminator::Normal
            | Terminator::Branch { .. }
            | Terminator::IfElse { .. }
            | Terminator::While { .. } => {}
        }
    }
}

impl<BlockIdentifier: BlockIdentifierTrait, BlockContent: BlockContentTrait>
    HyperBlock<BlockIdentifier, BlockContent>
{
    pub fn content_iter_mut(
        &mut self,
    ) -> Box<dyn Iterator<Item = &'_ mut BasicBlock<BlockIdentifier, BlockContent>> + '_> {
        match self {
            HyperBlock::ConnectedBlocks(blocks) => {
                Box::new(blocks.iter_mut().map(|x| x.inner_mut()))
            }

            HyperBlock::IfElseBlocks { if_unit, else_unit } => Box::new(
                if_unit
                    .inner_mut()
                    .content_iter_mut()
                    .chain(else_unit.inner_mut().content_iter_mut()),
            ),

            HyperBlock::WhileBlocks { inner, outer, .. } => Box::new(
                Box::new(inner.inner_mut().content_iter_mut())
                    .chain(outer.inner_mut().content_iter_mut()),
            ),
        }
    }

    pub fn content_iter(
        &self,
    ) -> Box<dyn Iterator<Item = &'_ BasicBlock<BlockIdentifier, BlockContent>> + '_> {
        match self {
            HyperBlock::ConnectedBlocks(blocks) => Box::new(blocks.iter().map(|x| x.inner())),
            HyperBlock::IfElseBlocks { if_unit, else_unit } => Box::new(
                if_unit
                    .inner()
                    .content_iter()
                    .chain(else_unit.inner().content_iter()),
            ),

            HyperBlock::WhileBlocks { inner, outer, .. } => {
                Box::new(Box::new(inner.inner().content_iter()).chain(outer.inner().content_iter()))
            }
        }
    }

    #[allow(dead_code)]
    fn display<Ctx: DecompileDisplayContext<BlockIdentifier, BlockContent>>(&self, ctx: &mut Ctx) {
        match self {
            HyperBlock::ConnectedBlocks(blocks) => {
                blocks.iter().for_each(|b| b.inner().display(ctx));
            }
            HyperBlock::IfElseBlocks { if_unit, else_unit } => {
                // ctx.add_lines("{");
                // the if condition is rendered in the previous block
                ctx.block(|ctx| {
                    if_unit.inner().display(ctx);
                });

                if !else_unit.inner().blocks.is_empty() {
                    ctx.add_lines("} else {");
                    ctx.block(|ctx| {
                        else_unit.inner().display(ctx);
                    });
                }

                ctx.add_lines("}");
            }

            HyperBlock::WhileBlocks {
                inner,
                outer,
                unconditional,
                ..
            } => {
                // ctx.add_lines("{");
                if *unconditional {
                    ctx.add_lines("loop {");
                }

                // the while condition is rendered in the previous block
                ctx.block(|ctx| {
                    inner.inner().display(ctx);
                });

                ctx.add_lines("}");

                outer.inner().display(ctx);
            }
        }
    }

    pub fn is_abort(&self) -> bool {
        match self {
            HyperBlock::ConnectedBlocks(blocks) => blocks
                .iter()
                .any(|block| matches!(block.inner().next, Terminator::Abort)),

            HyperBlock::IfElseBlocks { if_unit, else_unit } => {
                if_unit.inner().is_abort() && else_unit.inner().is_abort()
            }

            HyperBlock::WhileBlocks {
                inner,
                outer,
                unconditional,
                ..
            } => (*unconditional && inner.inner().is_abort()) || outer.inner().is_abort(),
        }
    }

    pub fn is_terminated(&self) -> bool {
        match self {
            HyperBlock::ConnectedBlocks(blocks) => blocks
                .iter()
                .any(|block| matches!(block.inner().next, Terminator::Ret | Terminator::Abort)),

            HyperBlock::IfElseBlocks { if_unit, else_unit } => {
                if_unit.inner().is_terminated() && else_unit.inner().is_terminated()
            }

            HyperBlock::WhileBlocks {
                inner,
                outer,
                unconditional,
                ..
            } => (*unconditional && inner.inner().is_terminated()) || outer.inner().is_terminated(),
        }
    }

    pub fn is_terminated_in_loop(&self) -> bool {
        match self {
            HyperBlock::ConnectedBlocks(blocks) => blocks
                .iter()
                .any(|block| block.inner().next.is_terminated_in_loop()),

            HyperBlock::IfElseBlocks { if_unit, else_unit } => {
                if_unit.inner().is_terminated_in_loop() && else_unit.inner().is_terminated_in_loop()
            }

            HyperBlock::WhileBlocks { outer, .. } => outer.inner().is_terminated_in_loop(),
        }
    }

    pub fn terminator(&self) -> Option<&Terminator<BlockIdentifier>> {
        match self {
            HyperBlock::ConnectedBlocks(blocks) => blocks.iter().find_map(|block| {
                if block.inner().next.is_terminated_in_loop() {
                    Some(&block.inner().next)
                } else {
                    None
                }
            }),

            HyperBlock::IfElseBlocks { .. } => {
                // if let Some(terminator) = if_body.terminator() {
                //     return terminator;
                // }
                // if let Some(terminator) = else_body.terminator() {
                //     return terminator;
                // }
                // Terminator::Normal
                None
            }

            HyperBlock::WhileBlocks { outer, .. } => outer.inner().terminator(),
        }
    }
}

impl<BlockIdentifier: BlockIdentifierTrait, BlockContent: BlockContentTrait>
    CodeUnitBlock<BlockIdentifier, BlockContent>
{
    pub fn terminator(&self) -> Option<&Terminator<BlockIdentifier>> {
        self.blocks
            .iter()
            .find_map(|block| block.inner().terminator())
    }

    pub fn is_abort(&self) -> bool {
        if let Some(last) = self.blocks.last() {
            return last.inner().is_abort();
        }
        false
    }

    pub fn is_terminated(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| block.inner().is_terminated())
    }

    pub fn is_terminated_in_loop(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| block.inner().is_terminated_in_loop())
    }

    pub fn content_iter_mut(
        &mut self,
    ) -> impl Iterator<Item = &'_ mut BasicBlock<BlockIdentifier, BlockContent>> {
        self.blocks
            .iter_mut()
            .flat_map(|b| b.inner_mut().content_iter_mut())
    }

    pub fn content_iter(&self) -> impl Iterator<Item = &BasicBlock<BlockIdentifier, BlockContent>> {
        self.blocks.iter().flat_map(|b| b.inner().content_iter())
    }

    pub fn display<Ctx: DecompileDisplayContext<BlockIdentifier, BlockContent>>(
        &self,
        ctx: &mut Ctx,
    ) {
        self.blocks.iter().for_each(|b| b.inner().display(ctx));
    }
}
