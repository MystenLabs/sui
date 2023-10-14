  use crate::{
      cfgir::optimize::constant_fold::{optimize_cmd, optimize_exp},
      hlir::ast::{self as H, Block, UnannotatedExp_ as E, Statement_ as S, Command_ as C, Value_},
  };
  use move_ir_types::location::*;


pub fn optimize(block: Block) -> Block {
    let mut output_block = Block::new();
    optimize_block(block, &mut output_block);
    output_block
}

fn optimize_block(block: Block, output_block: &mut Block) {
    for sp!(sloc, stmt) in block.into_iter().rev() {
        match stmt {
            S::IfElse { mut cond, if_block: in_if_block, else_block: in_else_block } => {
                optimize_exp(&mut cond);
                if is_true_bool(&cond) {
                    optimize_block(in_if_block, output_block)
                } else if is_false_bool(&cond) {
                    optimize_block(in_else_block, output_block)
                } else {
                    let mut if_block = Block::new(); 
                    optimize_block(in_if_block, &mut if_block);
                    let mut else_block = Block::new(); 
                    optimize_block(in_else_block, &mut else_block);
                    output_block.push_front(sp(sloc, S::IfElse { cond, if_block, else_block }));
                }
            }
            S::While { cond: (in_cond_block, mut cond_exp), block: in_body } => {
                optimize_exp(&mut cond_exp);
                if is_false_bool(&cond_exp) {
                    optimize_block(in_cond_block, output_block);
                } else {
                    let mut cond_block = Block::new();
                    optimize_block(in_cond_block, &mut cond_block);
                    let mut body = Block::new();
                    optimize_block(in_body, &mut body);
                    output_block.push_front(sp(sloc, S::While { cond: (cond_block, cond_exp), block: body }));
                }
            }
            S::Loop { block: in_body, .. } => {
                let mut body = Block::new();
                optimize_block(in_body, &mut body);
                let has_break = still_has_break(&body);
                if !has_break {
                    output_block.clear();
                }
                output_block.push_front(sp(sloc, S::Loop { block: body, has_break }));
            }
            S::Command(sp!(_, C::Assign(lvalues, exp))) if lvalues.len() == 0 && is_unit(&exp) => (),
            S::Command(mut cmd) => {
                if cmd.value.is_hlir_terminal() {
                    output_block.clear();
                }
                if let Some(_) = optimize_cmd(&mut cmd) {
                    output_block.push_front(sp(sloc, S::Command(cmd)));
                }
            }
        }
    }
}

//--------------------------------------------------------------------------------------------------
// Helperss
//--------------------------------------------------------------------------------------------------

fn is_unit(e: &H::Exp) -> bool {
    matches!(e.exp.value, E::Unit { .. })
}

fn is_true_bool(e: &H::Exp) -> bool {
    matches!(e.exp.value, E::Value(sp!(_, Value_::Bool(true))))
}

fn is_false_bool(e: &H::Exp) -> bool {
    matches!(e.exp.value, E::Value(sp!(_, Value_::Bool(false))))
}

fn still_has_break(block: &Block) -> bool {

    fn has_break(sp!(_, stmt_): &H::Statement) -> bool {
        match stmt_ {
            S::IfElse {
                if_block,
                else_block,
                ..
            } => has_break_block(if_block) || has_break_block(else_block),
            S::While { block, .. } => has_break_block(block),
            S::Loop { .. } => false,
            S::Command(sp!(_, H::Command_::Break)) => true,
            _ => false,
        }
    }

    fn has_break_block(block: &Block) -> bool {
        block.iter().any(has_break)
    }

    has_break_block(block)
}
