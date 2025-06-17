use crate::stackless::ast::Function;

mod inline_immediates;

pub fn optimize(function: &mut Function) {
    inline_immediates::optimize(function);
}
