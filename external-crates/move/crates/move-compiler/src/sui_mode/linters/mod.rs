// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::visitor::AbstractInterpreterVisitor,
    command_line::compiler::Visitor,
    diagnostics::{
        codes::{DiagnosticInfo, DiagnosticsID, Severity, custom},
        filter::FilterName,
    },
    expansion::ast as E,
    hlir::ast::{BaseType_, SingleType, SingleType_},
    linters::{ALLOW_ATTR_CATEGORY, LINT_WARNING_PREFIX, LintLevel, LinterDiagnosticCategory},
    typing::visitor::TypingVisitor,
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

pub mod coin_field;
pub mod collection_equality;
pub mod custom_state_change;
pub mod freeze_wrapped;
pub mod freezing_capability;
pub mod missing_key;
pub mod public_mut_tx_context;
pub mod public_random;
pub mod self_transfer;
pub mod share_owned;
pub mod uncallable_function;
pub mod unnecessary_public_entry;
pub mod unused_object_with_fields;

pub const TRANSFER_MOD_NAME: &str = "transfer";
pub const TRANSFER_FUN: &str = "transfer";
pub const PUBLIC_TRANSFER_FUN: &str = "public_transfer";
pub const SHARE_FUN: &str = "share_object";
pub const PUBLIC_SHARE_FUN: &str = "public_share_object";
pub const FREEZE_FUN: &str = "freeze_object";
pub const PUBLIC_FREEZE_FUN: &str = "public_freeze_object";
pub const RECEIVE_FUN: &str = "receive";
pub const PUBLIC_RECEIVE_FUN: &str = "public_receive";

pub const COIN_MOD_NAME: &str = "coin";
pub const COIN_STRUCT_NAME: &str = "Coin";

pub const BAG_MOD_NAME: &str = "bag";
pub const BAG_STRUCT_NAME: &str = "Bag";

pub const OBJECT_BAG_MOD_NAME: &str = "object_bag";
pub const OBJECT_BAG_STRUCT_NAME: &str = "ObjectBag";

pub const TABLE_MOD_NAME: &str = "table";
pub const TABLE_STRUCT_NAME: &str = "Table";

pub const OBJECT_TABLE_MOD_NAME: &str = "object_table";
pub const OBJECT_TABLE_STRUCT_NAME: &str = "ObjectTable";

pub const LINKED_TABLE_MOD_NAME: &str = "linked_table";
pub const LINKED_TABLE_STRUCT_NAME: &str = "LinkedTable";

pub const TABLE_VEC_MOD_NAME: &str = "table_vec";
pub const TABLE_VEC_STRUCT_NAME: &str = "TableVec";

pub const VEC_MAP_MOD_NAME: &str = "vec_map";
pub const VEC_MAP_STRUCT_NAME: &str = "VecMap";

pub const VEC_SET_MOD_NAME: &str = "vec_set";
pub const VEC_SET_STRUCT_NAME: &str = "VecSet";

pub const RANDOM_MOD_NAME: &str = "random";
pub const RANDOM_STRUCT_NAME: &str = "Random";
pub const RANDOM_GENERATOR_STRUCT_NAME: &str = "RandomGenerator";

pub const INVALID_LOC: Loc = Loc::invalid();

macro_rules! sui_lints {
    (
        $(
            ($enum_name:ident, $filter_name:expr, $code_msg:expr)
        ),* $(,)?
    ) => {
        // Unlike `StyleCodes`, there is no zero placeholder -- Sui lint codes start at 0 and are
        // positional: reordering or removing a variant changes the rendered `W99xxx` codes.
        #[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum LinterDiagnosticCode {
            $(
                $enum_name,
            )*
        }

        impl LinterDiagnosticCode {
            pub(crate) const fn diag_info(&self) -> DiagnosticInfo {
                let code = *self as u8;
                match self {
                    // A lint has an explanation iff
                    // `diagnostics/explanations/Sui_<CodeName>.md` exists -- keyed by
                    // identifiers rather than the rendered code since the numeric portion is
                    // positional (see `codes!` in diagnostics/codes.rs).
                    $(Self::$enum_name => custom(
                        LINT_WARNING_PREFIX,
                        Severity::Warning,
                        LinterDiagnosticCategory::Sui as u8,
                        code,
                        $code_msg,
                    ).with_explanation(move_proc_macros::optional_include_str!(
                        "src/diagnostics/explanations/", Sui, "_", $enum_name, ".md"
                    )),)*
                }
            }
        }

        /// Every Sui lint as `(filter name, diagnostic info)`, in code order.
        pub(crate) const SUI_LINTS: &[(&str, DiagnosticInfo)] = &[
            $(($filter_name, LinterDiagnosticCode::$enum_name.diag_info()),)*
        ];
    }
}

sui_lints!(
    (ShareOwned, "share_owned", "possible owned object share"),
    (
        SelfTransfer,
        "self_transfer",
        "non-composable transfer to sender"
    ),
    (
        CustomStateChange,
        "custom_state_change",
        "potentially unenforceable custom transfer/share/freeze policy"
    ),
    (
        CoinField,
        "coin_field",
        "sub-optimal 'sui::coin::Coin' field type"
    ),
    (
        FreezeWrapped,
        "freeze_wrapped",
        "attempting to freeze wrapped objects"
    ),
    (
        CollectionEquality,
        "collection_equality",
        "possibly useless collections compare"
    ),
    (PublicRandom, "public_random", "Risky use of 'sui::random'"),
    (
        MissingKey,
        "missing_key",
        "struct with id but missing key ability"
    ),
    (
        FreezingCapability,
        "freezing_capability",
        "freezing potential capability"
    ),
    (
        PreferMutableTxContext,
        "prefer_mut_tx_context",
        "prefer '&mut TxContext' over '&TxContext'"
    ),
    (
        UnnecessaryPublicEntry,
        "public_entry",
        "unnecessary `entry` on a `public` function"
    ),
    (
        UncallableFunction,
        "uncallable_function",
        "it will not be possible to call this function"
    ),
    (
        UnusedObjWithFields,
        "unused_object_with_fields",
        "unused object with fields"
    ),
);

pub fn known_filters() -> (Option<Symbol>, Vec<(FilterName, Vec<DiagnosticsID>)>) {
    // `lint(all)` is registered by the core linter (`linters::known_filters`); don't
    // register it again here or `filter_from_str` returns duplicate ids.
    let filters = SUI_LINTS
        .iter()
        .map(|(filter_name, info)| (Symbol::from(*filter_name), vec![info.id()]))
        .collect();
    (Some(ALLOW_ATTR_CATEGORY.into()), filters)
}

pub fn linter_visitors(level: LintLevel) -> Vec<Visitor> {
    match level {
        LintLevel::None => vec![],
        LintLevel::Default => vec![
            share_owned::ShareOwnedVerifier.visitor(),
            self_transfer::SelfTransferVerifier.visitor(),
            custom_state_change::CustomStateChangeVerifier.visitor(),
            coin_field::CoinFieldVisitor.visitor(),
            freeze_wrapped::FreezeWrappedVisitor.visitor(),
            collection_equality::CollectionEqualityVisitor.visitor(),
            public_random::PublicRandomVisitor.visitor(),
            missing_key::MissingKeyVisitor.visitor(),
            unnecessary_public_entry::UnnecessaryPublicEntry.visitor(),
            uncallable_function::UncallableFunction.visitor(),
            unused_object_with_fields::UnusedObjWithFieldsVerifier.visitor(),
            // This is not on by default outside of Sui mode
            crate::linters::unused_return_value::UnusedReturnValue.visitor(),
        ],
        LintLevel::All => {
            let mut visitors = linter_visitors(LintLevel::Default);
            visitors.extend([
                freezing_capability::WarnFreezeCapability.visitor(),
                public_mut_tx_context::PreferMutableTxContext.visitor(),
            ]);
            visitors
        }
    }
}

/// Returns abilities of a given type, if any.
pub fn type_abilities(sp!(_, st_): &SingleType) -> Option<E::AbilitySet> {
    let sp!(_, bt_) = match st_ {
        SingleType_::Base(v) => v,
        SingleType_::Ref(_, v) => v,
    };
    if let BaseType_::Apply(abilities, _, _) = bt_ {
        return Some(abilities.clone());
    }
    None
}
