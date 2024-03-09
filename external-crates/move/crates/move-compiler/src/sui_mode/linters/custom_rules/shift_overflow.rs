//! Detect potential overflow scenarios where the number of bits being shifted exceeds the bit width of
//! the variable being shifted, which could lead to unintended behavior or loss of data. If such a
//! potential overflow is detected, a warning is generated to alert the developer.
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
    sui_mode::linters::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX},
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;
use std::str::FromStr;

const SHIFT_OPERATION_OVERFLOW_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::ShiftOperationOverflow as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "Potential overflow detected. The number of bits being shifted exceeds the bit width of the variable being shifted.",
);

pub struct ShiftOperationOverflowVisitor;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for ShiftOperationOverflowVisitor {
    type Context<'a> = Context<'a>;

    fn context<'a>(
        env: &'a mut CompilationEnv,
        _program_info: &'a TypingProgramInfo,
        _program: &T::Program_,
    ) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(lhs, op, _, rhs) = &exp.exp.value {
            if matches!(op.value, BinOp_::Shl | BinOp_::Shr) {
                if let Some(bit_width) = get_bit_width(&lhs.ty.value) {
                    if let Some(shift_amount) = get_shift_amount(&rhs.exp.value) {
                        if shift_amount >= bit_width {
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

fn get_bit_width(ty: &Type_) -> Option<u128> {
    if let Type_::Apply(_, sp!(_, TypeName_::Builtin(sp!(_, typ))), _) = ty {
        match typ {
            BuiltinTypeName_::U8 => Some(8),
            BuiltinTypeName_::U16 => Some(16),
            BuiltinTypeName_::U32 => Some(32),
            BuiltinTypeName_::U64 => Some(64),
            BuiltinTypeName_::U128 => Some(128),
            BuiltinTypeName_::U256 => Some(256),
            _ => None,
        }
    } else {
        None
    }
}

fn get_shift_amount(value: &UnannotatedExp_) -> Option<u128> {
    if let UnannotatedExp_::Value(v) = value {
        match &v.value {
            Value_::U8(v) => Some(*v as u128),
            Value_::U16(v) => Some(*v as u128),
            Value_::U32(v) => Some(*v as u128),
            Value_::U64(v) => Some(*v as u128),
            Value_::U128(v) => Some(*v),
            Value_::InferredNum(v) | Value_::U256(v) => {
                let u256_val = move_core_types::u256::U256::from_str(&v.to_string()).ok()?;
                // Check if the U256 value can fit into a u128 by comparing it with the maximum u128 value.
                if u256_val <= move_core_types::u256::U256::from(u128::MAX) {
                    // Safely convert U256 to u128 by converting it to a string and then parsing it as u128.
                    // This step is safe because we already checked that the value doesn't exceed u128::MAX.
                    u128::from_str_radix(&u256_val.to_string(), 10).ok()
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    }
}

fn report_overflow(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        SHIFT_OPERATION_OVERFLOW_DIAG,
        (loc, "Potential overflow detected. The number of bits being shifted exceeds the bit width of the variable being shifted.")
    );
    env.add_diag(diag);
}
