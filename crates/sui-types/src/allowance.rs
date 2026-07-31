// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rust bindings for `sui::allowance`.
//!
//! Signing validates a tx's declared (funder, allowance) source against the
//! loaded object and reserves against the funder; execution creates the
//! `AllowanceWithdrawal<T>` that only the allowance's spend paths can unpack.

use crate::SUI_FRAMEWORK_ADDRESS;
use crate::base_types::{SuiAddress, move_utf8_str_layout, option_layout, type_name_layout};
use crate::error::{UserInputError, UserInputResult};
use crate::id::UID;
use crate::object::Object;
use crate::object::option_visitor::OptionVisitor;
use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_extractor::{Element, Extractor};
use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::u256::U256;
use move_core_types::visitor_default;
use mysten_common::debug_fatal;

pub const ALLOWANCE_MODULE_NAME: &IdentStr = ident_str!("allowance");
pub const ALLOWANCE_STRUCT_NAME: &IdentStr = ident_str!("Allowance");
pub const ALLOWANCE_WITHDRAWAL_STRUCT_NAME: &IdentStr = ident_str!("AllowanceWithdrawal");
pub const RESOLVED_ALLOWANCE_WITHDRAWAL_STRUCT: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    ALLOWANCE_MODULE_NAME,
    ALLOWANCE_WITHDRAWAL_STRUCT_NAME,
);

pub struct Allowance;

impl Allowance {
    pub fn type_(type_param: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ALLOWANCE_MODULE_NAME.to_owned(),
            name: ALLOWANCE_STRUCT_NAME.to_owned(),
            type_params: vec![type_param],
        }
    }

    pub fn is_allowance(s: &StructTag) -> bool {
        s.address == SUI_FRAMEWORK_ADDRESS
            && s.module.as_ident_str() == ALLOWANCE_MODULE_NAME
            && s.name.as_ident_str() == ALLOWANCE_STRUCT_NAME
            && s.type_params.len() == 1
    }

    /// Layout of `sui::allowance::Allowance<T>`. Only `funder` and `spender`
    /// are read, but the whole value must be described: the visitor rejects
    /// trailing bytes, so every field has to be skippable.
    pub fn layout(type_param: TypeTag) -> A::MoveStructLayout {
        A::MoveStructLayout {
            type_: Self::type_(type_param),
            fields: vec![
                field(
                    ident_str!("id"),
                    A::MoveTypeLayout::Struct(Box::new(UID::layout())),
                ),
                field(
                    ident_str!("settings"),
                    A::MoveTypeLayout::Struct(Box::new(settings_layout())),
                ),
                field(ident_str!("current_spend"), A::MoveTypeLayout::U256),
            ],
        }
    }
}

fn field(name: &IdentStr, layout: A::MoveTypeLayout) -> A::MoveFieldLayout {
    A::MoveFieldLayout::new(name.to_owned(), layout)
}

fn allowance_type(name: &IdentStr) -> StructTag {
    StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ALLOWANCE_MODULE_NAME.to_owned(),
        name: name.to_owned(),
        type_params: vec![],
    }
}

/// `sui::allowance::Window`. Both variants are positional, so their fields are
/// named `pos0`.
fn window_layout() -> A::MoveTypeLayout {
    A::MoveTypeLayout::Enum(Box::new(A::MoveEnumLayout {
        type_: allowance_type(ident_str!("Window")),
        variants: [
            (
                (ident_str!("PeriodicMs").to_owned(), 0),
                vec![field(ident_str!("pos0"), A::MoveTypeLayout::U64)],
            ),
            (
                (ident_str!("CalendarMonths").to_owned(), 1),
                vec![field(ident_str!("pos0"), A::MoveTypeLayout::U8)],
            ),
        ]
        .into_iter()
        .collect(),
    }))
}

/// `sui::allowance::RateLimit`.
fn rate_limit_layout() -> A::MoveTypeLayout {
    A::MoveTypeLayout::Enum(Box::new(A::MoveEnumLayout {
        type_: allowance_type(ident_str!("RateLimit")),
        variants: [(
            (ident_str!("Windowed").to_owned(), 0),
            vec![
                field(ident_str!("limit"), A::MoveTypeLayout::U256),
                field(ident_str!("spent"), A::MoveTypeLayout::U256),
                field(
                    ident_str!("anchor_ms"),
                    option_layout(TypeTag::U64, A::MoveTypeLayout::U64),
                ),
                field(ident_str!("index"), A::MoveTypeLayout::U64),
                field(ident_str!("window"), window_layout()),
            ],
        )]
        .into_iter()
        .collect(),
    }))
}

/// `sui::allowance::Settings`.
fn settings_layout() -> A::MoveStructLayout {
    let rate_limit_tag = TypeTag::Struct(Box::new(allowance_type(ident_str!("RateLimit"))));
    A::MoveStructLayout {
        type_: allowance_type(ident_str!("Settings")),
        fields: vec![
            field(ident_str!("funder"), A::MoveTypeLayout::Address),
            field(
                ident_str!("spender"),
                option_layout(TypeTag::Address, A::MoveTypeLayout::Address),
            ),
            field(ident_str!("app"), {
                let type_name = type_name_layout();
                let tag = TypeTag::Struct(Box::new(type_name.type_.clone()));
                option_layout(tag, A::MoveTypeLayout::Struct(Box::new(type_name)))
            }),
            field(
                ident_str!("lifetime_cap"),
                option_layout(TypeTag::U256, A::MoveTypeLayout::U256),
            ),
            field(
                ident_str!("start_timestamp_ms"),
                option_layout(TypeTag::U64, A::MoveTypeLayout::U64),
            ),
            field(
                ident_str!("expiration_timestamp_ms"),
                option_layout(TypeTag::U64, A::MoveTypeLayout::U64),
            ),
            field(
                ident_str!("rate_limit"),
                option_layout(rate_limit_tag, rate_limit_layout()),
            ),
            field(
                ident_str!("name"),
                A::MoveTypeLayout::Struct(Box::new(move_utf8_str_layout())),
            ),
        ],
    }
}

#[derive(thiserror::Error, Debug)]
pub enum VisitError {
    #[error("Unexpected type in allowance")]
    UnexpectedType,

    #[error(transparent)]
    Visitor(#[from] AV::Error),
}

impl From<crate::object::option_visitor::Error> for VisitError {
    fn from(_: crate::object::option_visitor::Error) -> Self {
        Self::UnexpectedType
    }
}

/// The layout fixes these fields as addresses, so the non-address arms below are unreachable;
/// they error rather than yield nothing because this runs on the signing path.
struct AddressVisitor;

impl<'b, 'l> AV::Visitor<'b, 'l> for AddressVisitor {
    type Value = SuiAddress;
    type Error = VisitError;

    visitor_default! { <'b, 'l> u8, u16, u32, u64, u128, u256 = Err(VisitError::UnexpectedType) }
    visitor_default! { <'b, 'l> bool, signer, vector, struct, variant = Err(VisitError::UnexpectedType) }

    fn visit_address(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(SuiAddress::from(value))
    }
}

/// The policy fields are skipped: they are only enforced in Move.
struct SettingsVisitor;

impl<'b, 'l> AV::Visitor<'b, 'l> for SettingsVisitor {
    type Value = (SuiAddress, Option<SuiAddress>);
    type Error = VisitError;

    visitor_default! { <'b, 'l> u8, u16, u32, u64, u128, u256 = Err(VisitError::UnexpectedType) }
    visitor_default! { <'b, 'l> bool, address, signer, vector, variant = Err(VisitError::UnexpectedType) }

    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        let mut funder = None;
        let mut spender = None;
        while let Some(f) = driver.peek_field() {
            match f.name.as_str() {
                "funder" => {
                    funder = driver.next_field(&mut AddressVisitor)?.map(|(_, v)| v);
                }
                "spender" => {
                    spender = driver
                        .next_field(&mut OptionVisitor(&mut AddressVisitor))?
                        .map(|(_, v)| v);
                }
                _ => {
                    driver.skip_field()?;
                }
            }
        }
        // Both fields must have been visited; a missing one means the layout drifted. The
        // inner `None` is distinct and legitimate: a spender that is not set yet.
        let funder = funder.ok_or(VisitError::UnexpectedType)?;
        let spender = spender.ok_or(VisitError::UnexpectedType)?;
        Ok((funder, spender))
    }
}

/// Sign-time view of an `Allowance<T>`. The spender can rotate, so never
/// reuse a resolution across transactions.
#[derive(Debug, Clone)]
pub struct ResolvedAllowance {
    pub funder: SuiAddress,
    pub spender: Option<SuiAddress>,
    /// The accumulated type `T` of `Allowance<T>` (e.g. `Balance<SUI>`).
    pub funds_type: TypeTag,
}

/// Parses an object as an `Allowance`, extracting the sign-time-relevant fields.
pub fn parse_allowance_object(object: &Object) -> UserInputResult<ResolvedAllowance> {
    let invalid = |error: String| UserInputError::InvalidWithdrawReservation { error };
    let id = object.id();
    let Some(move_obj) = object.data.try_as_move() else {
        return Err(invalid(format!(
            "Specified allowance {id} is not a Move object"
        )));
    };
    let tag: StructTag = move_obj.type_().clone().into();
    if !Allowance::is_allowance(&tag) {
        return Err(invalid(format!(
            "Specified allowance {id} is not a sui::allowance::Allowance"
        )));
    }
    if !object.owner.is_shared() {
        return Err(invalid(format!("Allowance {id} is not a shared object")));
    }
    let funds_type = tag
        .type_params
        .into_iter()
        .next()
        .expect("checked by is_allowance");

    let layout = A::MoveTypeLayout::Struct(Box::new(Allowance::layout(funds_type.clone())));
    // Move wrote these bytes and the checks above pinned the type, so a decode failure means
    // `Allowance::layout` has drifted from the Move source. Not a panic: this runs at signing.
    let Ok(Some((funder, spender))) = Extractor::deserialize_value(
        move_obj.contents(),
        &layout,
        &mut SettingsVisitor,
        vec![Element::Field("settings")],
    ) else {
        debug_fatal!("allowance {id} did not match Allowance::layout");
        return Err(invalid(format!("Failed to read allowance {id}")));
    };

    Ok(ResolvedAllowance {
        funder,
        spender,
        funds_type,
    })
}
