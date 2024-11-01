//! Detects an unnecessary unit expression in a block, sequence, if, or else.

use crate::{
    diag, ice,
    linters::StyleCodes,
    typing::{
        ast::{self as T, SequenceItem_, UnannotatedExp_},
        visitor::simple_visitor,
    },
};
use move_ir_types::location::Loc;

simple_visitor!(
    UnnecessaryUnit,
    fn visit_seq_custom(&mut self, loc: Loc, (_, seq_): &T::Sequence) -> bool {
        let n = seq_.len();
        match n {
            0 => {
                self.add_diag(ice!((loc, "Unexpected empty block without a value")));
            }
            1 => {
                // TODO probably too noisy for now, we would need more information about
                // blocks were added by the programmer
                // self.env.add_diag(diag!(
                //     StyleCodes::UnnecessaryBlock.diag_info(),
                //     (e.exp.loc, "Unnecessary block expression '{}')"
                //     (e.exp.loc, if_msg),
                // ));
            }
            n => {
                let last = n - 1;
                for (i, stmt) in seq_.iter().enumerate() {
                    if i != last && is_unit_seq(self, stmt) {
                        let msg = "Unnecessary unit in sequence '();'. Consider removing";
                        self.add_diag(diag!(
                            StyleCodes::UnnecessaryUnit.diag_info(),
                            (stmt.loc, msg),
                        ));
                    }
                }
            }
        }
        false
    },
    fn visit_exp_custom(&mut self, e: &T::Exp) -> bool {
        use UnannotatedExp_ as TE;
        let TE::IfElse(e_cond, e_true, e_false_opt) = &e.exp.value else {
            return false;
        };
        if is_unit(self, e_true) {
            let u_msg = "Unnecessary unit '()'";
            let if_msg = "Consider negating the 'if' condition and simplifying";
            let mut diag = diag!(
                StyleCodes::UnnecessaryUnit.diag_info(),
                (e_true.exp.loc, u_msg),
                (e_cond.exp.loc, if_msg),
            );
            diag.add_note("For example 'if (cond) () else e' can be simplified to 'if (!cond) e'");
            self.add_diag(diag);
        }
        if let Some(e_false) = e_false_opt {
            if is_unit(self, e_false) {
                let u_msg = "Unnecessary 'else ()'.";
                let if_msg = "An 'if' without an 'else' has an implicit 'else ()'. \
                            Consider removing the 'else' branch";
                let mut diag = diag!(
                    StyleCodes::UnnecessaryUnit.diag_info(),
                    (e_false.exp.loc, u_msg),
                    (e.exp.loc, if_msg),
                );
                diag.add_note(
                    "For example 'if (cond) e else ()' can be simplified to 'if (cond) e'",
                );
                self.add_diag(diag);
            }
        }
        false
    }
);

fn is_unit_seq(context: &mut Context, s: &T::SequenceItem) -> bool {
    match &s.value {
        SequenceItem_::Seq(e) => is_unit(context, e),
        SequenceItem_::Declare(_) | SequenceItem_::Bind(_, _, _) => false,
    }
}

fn is_unit(context: &mut Context, e: &T::Exp) -> bool {
    use UnannotatedExp_ as TE;
    match &e.exp.value {
        TE::Unit { .. } => true,
        TE::Annotate(inner, _) => is_unit(context, inner),
        TE::Block((_, seq)) if seq.is_empty() => {
            context.add_diag(ice!((e.exp.loc, "Unexpected empty block without a value")));
            false
        }
        TE::Block((_, seq)) if seq.len() == 1 => is_unit_seq(context, &seq[0]),
        _ => false,
    }
}
