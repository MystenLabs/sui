//! Detects an unnecessary unit expression in a block, sequence, if, or else.
use crate::{
    diag,
    diagnostics::WarningFilters,
    linters::StyleCodes,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, SequenceItem_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};

pub struct UnnecessaryUnit;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for UnnecessaryUnit {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_exp_custom(&mut self, e: &T::Exp) -> bool {
        use UnannotatedExp_ as TE;
        match &e.exp.value {
            TE::IfElse(_, e_true, e_false) => {
                if is_unit(e_true) {
                    let u_msg = "Unnecessary unit '()'";
                    let if_msg = "Consider negating the 'if' condition and removing this case, \
                        e.g. 'if (cond) () else e' becomes 'if (!cond) e'";
                    self.env.add_diag(diag!(
                        StyleCodes::UnnecessaryUnit.diag_info(),
                        (e_true.exp.loc, u_msg),
                        (e.exp.loc, if_msg),
                    ));
                }
                if is_unit(e_false) {
                    let u_msg = "Unnecessary unit '()'";
                    let if_msg = "Unnecessary 'else ()'. \
                        An 'if' without an 'else' has an implicit 'else' with '()'. \
                        Consider removing, e.g. 'if (cond) e else ()' becomes 'if (cond) e'";
                    self.env.add_diag(diag!(
                        StyleCodes::UnnecessaryUnit.diag_info(),
                        (e_true.exp.loc, u_msg),
                        (e.exp.loc, if_msg),
                    ));
                }
            }
            TE::Block((_, seq_)) => {
                let n = seq_.len();
                match n {
                    0 | 1 => {
                        // TODO probably too noisy for now, we would need more information about
                        // blocks were added by the programmer
                        // self.env.add_diag(diag!(
                        //     StyleCodes::UnnecessaryBlock.diag_info(),
                        //     (e.exp.loc, "Unnecessary block expression '{}')"
                        //     (e.exp.loc, if_msg),
                        // ));
                    }
                    n => {
                        for (i, stmt) in seq_.iter().enumerate() {
                            if i != n && is_unit_seq(stmt) {
                                let msg = "Unnecessary unit in sequence '();'. Consider removing";
                                self.env.add_diag(diag!(
                                    StyleCodes::UnnecessaryUnit.diag_info(),
                                    (stmt.loc, msg),
                                ));
                            }
                        }
                    }
                }
            }
            _ => (),
        }
        false
    }
}

fn is_unit_seq(s: &T::SequenceItem) -> bool {
    match &s.value {
        SequenceItem_::Seq(e) => is_unit(e),
        SequenceItem_::Declare(_) | SequenceItem_::Bind(_, _, _) => false,
    }
}

fn is_unit(e: &T::Exp) -> bool {
    use UnannotatedExp_ as TE;
    match &e.exp.value {
        TE::Unit { .. } => true,
        TE::Annotate(inner, _) => is_unit(inner),
        TE::Block((_, seq)) if seq.len() == 1 => is_unit_seq(&seq[0]),
        _ => false,
    }
}
