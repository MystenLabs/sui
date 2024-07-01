//! This linter rule detects potential overflow in multiplication operations across various integer types.
//! It handles both same-type and mixed-type multiplications, issuing warnings when overflow is possible.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    naming::ast::{BuiltinTypeName_, TypeName_, Type_},
    parser::ast::BinOp_,
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;
use std::str::FromStr;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, MULTIPLICATION_OVERFLOW_DIAG_CODE};

const SHIFT_OPERATION_OVERFLOW_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Correctness as u8,
    MULTIPLICATION_OVERFLOW_DIAG_CODE,
    "Potential overflow detected in multiplication operation",
);

pub struct MultiplicationOverflow;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for MultiplicationOverflow {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(lhs, op, _, rhs) = &exp.exp.value {
            if matches!(op.value, BinOp_::Mul) {
                // First, check if both operands are of integer type and determine their sizes
                if let (Some(lhs_type), Some(rhs_type)) = (
                    get_integer_type(&lhs.ty.value),
                    get_integer_type(&rhs.ty.value),
                ) {
                    // Assuming a function to estimate the max value of each operand
                    if let (Some(lhs_max), Some(rhs_max)) =
                        (estimate_value(lhs), estimate_value(rhs))
                    {
                        // Check for potential overflow based on the max values and the types
                        let potential_overflow =
                            check_overflow_potential(lhs_type, rhs_type, lhs_max, rhs_max);
                        if potential_overflow {
                            report_overflow(self.env, op.loc);
                        }
                    }
                }
            }
        }
        false
    }
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }
}

// Helper function to get the integer type and size of an expression, if applicable
fn get_integer_type(exp_type: &Type_) -> Option<&'static str> {
    match exp_type {
        Type_::Apply(_, sp!(_, TypeName_::Builtin(sp!(_, typ))), _) => match typ {
            BuiltinTypeName_::U8 => Some("u8"),
            BuiltinTypeName_::U16 => Some("u64"),
            BuiltinTypeName_::U32 => Some("u64"),
            BuiltinTypeName_::U64 => Some("u64"),
            BuiltinTypeName_::U128 => Some("u128"),
            BuiltinTypeName_::U256 => Some("u256"),
            _ => None,
        },
        _ => None,
    }
}

// Placeholder function to estimate the maximum value an expression can hold
// This could be based on static analysis or safe upper bounds for the types
fn estimate_value(exp: &T::Exp) -> Option<u128> {
    match &exp.exp.value {
        UnannotatedExp_::Value(v) => match &v.value {
            Value_::U8(v) => Some(*v as u128),
            Value_::U16(v) => Some(*v as u128),
            Value_::U32(v) => Some(*v as u128),
            Value_::U64(v) => Some(*v as u128),
            Value_::U128(v) => Some(*v),
            Value_::InferredNum(v) | Value_::U256(v) => {
                let u256_val = move_core_types::u256::U256::from_str(&v.to_string()).ok()?;
                if u256_val > u128::MAX.into() {
                    None
                } else {
                    Some(u256_val.unchecked_as_u128())
                }
            }
            _ => None,
        },

        _ => None,
    }
}

// Function to check if the multiplication of two max values of given types could overflow
fn check_overflow_potential(
    lhs_type: &str,
    rhs_type: &str,
    lhs_value: u128,
    rhs_value: u128,
) -> bool {
    // Here, we assume if both operands are of the same type, we check based on that type's max value.
    let max_value = match (lhs_type, rhs_type) {
        ("u8", "u8") => u8::MAX as u128,
        ("u16", "u16") => u16::MAX as u128,
        ("u32", "u32") => u32::MAX as u128,
        ("u64", "u64") => u64::MAX as u128,
        ("u128", "u128") => u128::MAX,
        _ => u128::MAX,
    };
    lhs_value
        .checked_mul(rhs_value)
        .map_or(true, |result| result > max_value)
}

fn report_overflow(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        SHIFT_OPERATION_OVERFLOW_DIAG,
        (
            loc,
            "Potential overflow detected in multiplication operation"
        )
    );
    env.add_diag(diag);
}
