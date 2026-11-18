// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveStructLayout, MoveTypeLayout, MoveValue},
    annotated_visitor as AV,
    u256::U256,
    visitor_default,
};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use sui_types::{
    TypeTag,
    balance::Balance,
    base_types::{ObjectID, SuiAddress},
    id::UID,
    object::MoveObject,
};

/// Summed `Balance<T>` values, keyed by owning address (or object ID) and then by `T`.
pub type BalanceTotals = BTreeMap<SuiAddress, BTreeMap<Rc<TypeTag>, u128>>;

/// The UIDs and `Balance<T>` totals found in an object, used to find the addresses and balances a
/// transaction touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedContents {
    pub uids: BTreeSet<ObjectID>,
    /// Keyed by the innermost object (struct whose first field is a `UID`) enclosing each balance.
    pub balances: BalanceTotals,
}

/// Balances and contained UIDs of the objects a transaction read from storage.
#[derive(Debug)]
pub struct BalancesBefore {
    walked: BTreeMap<ObjectID, TrackedContents>,
}

/// The `Balance<T>` totals of a user transaction before and after execution, and the addresses
/// involved in it.
///
/// `before` covers every object read and `after` only the objects written, so the two are not
/// comparable. Both see the gas coin before the gas charge, which is not tracked since SUI cannot
/// be regulated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceTracking {
    pub before: BalanceTotals,
    pub after: BalanceTotals,
    pub addresses: BTreeSet<SuiAddress>,
}

/// Adds every total in `from` to `into`, failing on overflow.
pub fn merge_totals(into: &mut BalanceTotals, from: &BalanceTotals) -> Result<(), String> {
    for (owner, by_type) in from {
        for (type_, value) in by_type {
            add_to_totals(into, *owner, type_, *value)?;
        }
    }
    Ok(())
}

/// Adds `amount` to the `(owner, type_)` total, failing on overflow.
pub fn add_to_totals(
    totals: &mut BalanceTotals,
    owner: SuiAddress,
    type_: &Rc<TypeTag>,
    amount: u128,
) -> Result<(), String> {
    let total = totals
        .entry(owner)
        .or_default()
        .entry(type_.clone())
        .or_insert(0);
    *total = total
        .checked_add(amount)
        .ok_or_else(|| format!("Balance total overflow for {owner} and type {type_}"))?;
    Ok(())
}

impl TrackedContents {
    pub(crate) fn new() -> Self {
        Self {
            uids: BTreeSet::new(),
            balances: BalanceTotals::new(),
        }
    }
}

impl BalancesBefore {
    pub(crate) fn new() -> Self {
        Self {
            walked: BTreeMap::new(),
        }
    }

    /// Records the contents of `id` as read from storage. A repeated read must match the earlier one.
    pub fn record(&mut self, id: ObjectID, contents: TrackedContents) {
        debug_assert!(
            self.walked.get(&id).is_none_or(|prev| *prev == contents),
            "Object {id} read twice with different contents"
        );
        self.walked.insert(id, contents);
    }

    /// Returns the before totals and every UID found in the walked objects.
    pub fn finish(self) -> Result<(BalanceTotals, BTreeSet<ObjectID>), String> {
        let mut totals = BalanceTotals::new();
        let mut uids = BTreeSet::new();
        for contents in self.walked.into_values() {
            merge_totals(&mut totals, &contents.balances)?;
            uids.extend(contents.uids);
        }
        Ok((totals, uids))
    }
}

/// The UIDs in `obj` and, if `track_balances`, its `Balance<T>` totals.
///
/// Coins are read without a layout. Otherwise `layout` is called, and returning `None` skips the
/// walk; callers that need the UIDs must always return a layout.
pub fn object_contents<L: Borrow<MoveTypeLayout>, E>(
    obj: &MoveObject,
    track_balances: bool,
    layout: impl FnOnce() -> Result<Option<L>, E>,
) -> Result<Result<TrackedContents, /* invariant violation */ String>, E> {
    if obj.type_().is_coin() {
        let id = obj.id();
        let mut balances = BalanceTotals::new();
        if track_balances && let Some(type_) = obj.type_().coin_type_maybe() {
            let value = u128::from(obj.get_coin_value_unsafe());
            balances.insert(id.into(), BTreeMap::from([(Rc::new(type_), value)]));
        }
        let uids = BTreeSet::from([id]);
        return Ok(Ok(TrackedContents { uids, balances }));
    }
    let Some(layout) = layout()? else {
        return Ok(Ok(TrackedContents::new()));
    };
    Ok(walk_contents(
        layout.borrow(),
        obj.contents(),
        track_balances,
    ))
}

/// True if a value of this layout can hold a `Balance<T>`.
pub fn layout_contains_balance(layout: &MoveTypeLayout) -> bool {
    let mut stack = vec![layout];
    while let Some(layout) = stack.pop() {
        match layout {
            MoveTypeLayout::Struct(s) => {
                if Balance::is_balance_layout(s) {
                    return true;
                }
                stack.extend(s.fields.iter().map(|f| &f.layout));
            }
            MoveTypeLayout::Enum(e) => {
                stack.extend(e.variants.values().flatten().map(|f| &f.layout));
            }
            MoveTypeLayout::Vector(inner) => stack.push(inner),
            MoveTypeLayout::Bool
            | MoveTypeLayout::U8
            | MoveTypeLayout::U16
            | MoveTypeLayout::U32
            | MoveTypeLayout::U64
            | MoveTypeLayout::U128
            | MoveTypeLayout::U256
            | MoveTypeLayout::Address
            | MoveTypeLayout::Signer => {}
        }
    }
    false
}

/// Walks a value, collecting every UID and, if `track_balances`, every `Balance<T>` summed per
/// innermost enclosing object.
///
/// WARNING! This function assumes that the bcs bytes have already been validated,
/// and it will give an invariant violation otherwise.
/// In short, we are relying on the invariant that the bytes are valid for objects
/// in storage.  We do not need this invariant for dev-inspect, as the programmable
/// transaction execution will validate the bytes before we get to this point.
pub fn walk_contents(
    fully_annotated_layout: &MoveTypeLayout,
    bcs_bytes: &[u8],
    track_balances: bool,
) -> Result<TrackedContents, /* invariant violation */ String> {
    let mut traversal = ContentsTraversal {
        contents: TrackedContents::new(),
        types: BTreeSet::new(),
        track_balances,
        enclosing_object: None,
    };
    MoveValue::visit_deserialize(bcs_bytes, fully_annotated_layout, &mut traversal)
        .map_err(|e| e.to_string())?;
    Ok(traversal.contents)
}

// -------------------------------------------------------------------------------------------------
// Traversal internals
// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
enum TraversalError {
    Visitor(AV::Error),
    Invariant(String),
}

impl std::fmt::Display for TraversalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Visitor(e) => write!(f, "Failed to deserialize. {e}"),
            Self::Invariant(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TraversalError {}

impl From<AV::Error> for TraversalError {
    fn from(e: AV::Error) -> Self {
        Self::Visitor(e)
    }
}

struct ContentsTraversal {
    contents: TrackedContents,
    /// Interns balance types so each distinct `T` is allocated once per walk.
    types: BTreeSet<Rc<TypeTag>>,
    track_balances: bool,
    enclosing_object: Option<ObjectID>,
}

/// Reads the address in a `UID` or `ID`.
struct UIDValue;

/// Reads the `u64` in a `Balance<T>`.
struct BalanceValue;

fn is_object_layout(layout: &MoveStructLayout) -> bool {
    layout
        .fields
        .first()
        .is_some_and(|f| matches!(&f.layout, MoveTypeLayout::Struct(s) if UID::is_uid(&s.type_)))
}

impl ContentsTraversal {
    fn visit_fields(
        &mut self,
        driver: &mut AV::StructDriver<'_, '_, '_>,
    ) -> Result<(), TraversalError> {
        while driver.next_field(self)?.is_some() {}
        Ok(())
    }

    /// Records the `UID` in the next field of `driver`, which may be the `UID` itself or an object.
    fn read_uid(
        &mut self,
        driver: &mut AV::StructDriver<'_, '_, '_>,
    ) -> Result<ObjectID, TraversalError> {
        let Some((_, id)) = driver.next_field(&mut UIDValue)? else {
            return Err(TraversalError::Invariant("UID has no address".to_string()));
        };
        self.contents.uids.insert(id);
        Ok(id)
    }

    fn visit_balance(
        &mut self,
        driver: &mut AV::StructDriver<'_, '_, '_>,
        type_: &TypeTag,
    ) -> Result<(), TraversalError> {
        let Some((_, value)) = driver.next_field(&mut BalanceValue)? else {
            return Err(TraversalError::Invariant(
                "Balance has no value".to_string(),
            ));
        };
        let Some(object) = self.enclosing_object else {
            return Err(TraversalError::Invariant(
                "Balance found outside of an object".to_string(),
            ));
        };
        let type_ = self.intern(type_);
        add_to_totals(
            &mut self.contents.balances,
            object.into(),
            &type_,
            u128::from(value),
        )
        .map_err(TraversalError::Invariant)
    }

    /// Balances in the object's fields count toward the object, not its enclosing one.
    fn visit_object(
        &mut self,
        driver: &mut AV::StructDriver<'_, '_, '_>,
    ) -> Result<(), TraversalError> {
        let id = self.read_uid(driver)?;
        let outer = self.enclosing_object.replace(id);
        let result = self.visit_fields(driver);
        self.enclosing_object = outer;
        result
    }

    fn intern(&mut self, type_: &TypeTag) -> Rc<TypeTag> {
        if let Some(interned) = self.types.get(type_) {
            return interned.clone();
        }
        let interned = Rc::new(type_.clone());
        self.types.insert(interned.clone());
        interned
    }
}

impl<'b, 'l> AV::Traversal<'b, 'l> for ContentsTraversal {
    type Error = TraversalError;

    fn traverse_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'b, 'l>,
    ) -> Result<(), Self::Error> {
        let layout = driver.struct_layout();
        if UID::is_uid(&layout.type_) {
            self.read_uid(driver)?;
            return Ok(());
        }
        if !self.track_balances {
            return self.visit_fields(driver);
        }
        if Balance::is_balance_layout(layout) {
            let Some(type_) = layout.type_.type_params.first() else {
                return Err(TraversalError::Invariant(
                    "Balance type has no type argument".to_string(),
                ));
            };
            return self.visit_balance(driver, type_);
        }
        if is_object_layout(layout) {
            return self.visit_object(driver);
        }
        self.visit_fields(driver)
    }
}

impl<'b, 'l> AV::Visitor<'b, 'l> for UIDValue {
    type Value = ObjectID;
    type Error = TraversalError;

    visitor_default! {
        <'b, 'l> u8, u16, u32, u64, u128, u256, bool, signer, vector, variant =
            Err(TraversalError::Invariant("Expected a UID address".to_string()))
    }

    fn visit_address(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(value.into())
    }

    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        driver
            .next_field(self)?
            .map(|(_, id)| id)
            .ok_or_else(|| TraversalError::Invariant("UID has no address".to_string()))
    }
}

impl<'b, 'l> AV::Visitor<'b, 'l> for BalanceValue {
    type Value = u64;
    type Error = TraversalError;

    visitor_default! {
        <'b, 'l> u8, u16, u32, u128, u256, bool, address, signer, vector, struct, variant =
            Err(TraversalError::Invariant("Expected a u64 balance value".to_string()))
    }

    fn visit_u64(
        &mut self,
        _driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error> {
        Ok(value)
    }
}
