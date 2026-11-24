// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Compiles [`OwnedObjectFilter`]s and [`DynamicFieldFilter`]s into the
//! `object_by_owner` keys they select, and scans only those keys.
//!
//! Every predicate constrains the Move type that follows the owner in an
//! `object_by_owner` key, so it selects the keys under a byte prefix, or a
//! union and difference of prefixes for a dynamic field's name type. AND, OR
//! and NOT of such sets are again sets of byte ranges, so a filter compiles
//! to sorted, disjoint ranges. [`FilteredScan`] bounds its RocksDB iterator to
//! the span of those ranges and seeks over each gap between them, so it reads
//! at most one rejected key per gap and returns matches in index order.

use std::ops::Bound;

use move_core_types::language_storage::StructTag;
use sui_consistent_store::DbMap;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Iter;
use sui_consistent_store::RevIter;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::iter::next_prefix;
use sui_consistent_store::reader::Reader;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::dynamic_field::DYNAMIC_FIELD_FIELD_STRUCT_NAME;
use sui_types::dynamic_field::DYNAMIC_FIELD_MODULE_NAME;
use sui_types::dynamic_field::DYNAMIC_OBJECT_FIELD_MODULE_NAME;
use sui_types::dynamic_field::DYNAMIC_OBJECT_FIELD_WRAPPER_STRUCT_NAME;
use sui_types::storage::DnfFilter;
use sui_types::storage::DynamicFieldFilter;
use sui_types::storage::DynamicFieldPredicate;
use sui_types::storage::NameTypePattern;
use sui_types::storage::OwnedObjectFilter;

use crate::schema::object_by_owner::OwnerKind;
use crate::schema::type_filter::TypeFilter;

/// BCS variant index of `TypeTag::Struct`.
const TYPE_TAG_STRUCT_VARIANT: u8 = 7;

/// The half-open byte range `[start, end)`; an `end` of `None` is unbounded.
#[derive(Clone, Debug, PartialEq, Eq)]
struct KeyRange {
    start: Vec<u8>,
    end: Option<Vec<u8>>,
}

/// A set of keys, as sorted ranges that neither overlap nor touch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeyRanges(Vec<KeyRange>);

/// Whether `key` sorts before the exclusive bound `end`.
fn before_end(key: &[u8], end: &Option<Vec<u8>>) -> bool {
    end.as_deref().is_none_or(|end| key < end)
}

/// Whether the exclusive bound `a` sorts at or before the exclusive bound `b`.
fn end_at_or_before(a: &Option<Vec<u8>>, b: &Option<Vec<u8>>) -> bool {
    match (a, b) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(a), Some(b)) => a <= b,
    }
}

impl KeyRanges {
    pub(crate) fn empty() -> Self {
        Self(Vec::new())
    }

    /// The entire key space.
    pub(crate) fn all() -> Self {
        Self(vec![KeyRange {
            start: Vec::new(),
            end: None,
        }])
    }

    /// Every key that starts with `prefix`.
    pub(crate) fn prefix(prefix: Vec<u8>) -> Self {
        let end = next_prefix(&prefix);
        Self(vec![KeyRange { start: prefix, end }])
    }

    /// Restrict this set to keys within the byte interval `(lower, upper)`.
    pub(crate) fn restrict(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>) -> Self {
        let start = match lower {
            Bound::Unbounded => Vec::new(),
            Bound::Included(bytes) => bytes.to_vec(),
            Bound::Excluded(bytes) => {
                let mut next = Vec::with_capacity(bytes.len() + 1);
                next.extend_from_slice(bytes);
                next.push(0);
                next
            }
        };
        let end = match upper {
            Bound::Unbounded => None,
            Bound::Excluded(bytes) => Some(bytes.to_vec()),
            Bound::Included(bytes) => {
                let mut next = Vec::with_capacity(bytes.len() + 1);
                next.extend_from_slice(bytes);
                next.push(0);
                Some(next)
            }
        };
        if !before_end(&start, &end) {
            return Self::empty();
        }
        self.intersection(&Self(vec![KeyRange { start, end }]))
    }

    fn union(self, other: Self) -> Self {
        let mut ranges: Vec<KeyRange> = self.0.into_iter().chain(other.0).collect();
        ranges.sort_by(|a, b| a.start.cmp(&b.start));
        let mut merged: Vec<KeyRange> = Vec::with_capacity(ranges.len());
        for range in ranges {
            match merged.last_mut() {
                // Overlapping or touching: extend the previous range.
                Some(last)
                    if last
                        .end
                        .as_deref()
                        .is_none_or(|end| range.start.as_slice() <= end) =>
                {
                    if !end_at_or_before(&range.end, &last.end) {
                        last.end = range.end;
                    }
                }
                _ => merged.push(range),
            }
        }
        Self(merged)
    }

    fn intersection(&self, other: &Self) -> Self {
        let (mut i, mut j) = (0, 0);
        let mut ranges = Vec::new();
        while let (Some(a), Some(b)) = (self.0.get(i), other.0.get(j)) {
            let start = a.start.as_slice().max(b.start.as_slice());
            let a_ends_first = end_at_or_before(&a.end, &b.end);
            let end = if a_ends_first { &a.end } else { &b.end };
            if before_end(start, end) {
                ranges.push(KeyRange {
                    start: start.to_vec(),
                    end: end.clone(),
                });
            }
            if a_ends_first {
                i += 1;
            } else {
                j += 1;
            }
        }
        Self(ranges)
    }

    fn complement(&self) -> Self {
        let mut ranges = Vec::new();
        // The empty byte string is the lowest key.
        let mut gap_start = Some(Vec::new());
        for range in &self.0 {
            let Some(start) = gap_start.take() else {
                break;
            };
            if start < range.start {
                ranges.push(KeyRange {
                    start,
                    end: Some(range.start.clone()),
                });
            }
            gap_start = range.end.clone();
        }
        if let Some(start) = gap_start {
            ranges.push(KeyRange { start, end: None });
        }
        Self(ranges)
    }

    fn difference(&self, other: &Self) -> Self {
        self.intersection(&other.complement())
    }

    #[cfg(test)]
    fn contains(&self, key: &[u8]) -> bool {
        self.0
            .iter()
            .any(|range| range.start.as_slice() <= key && before_end(key, &range.end))
    }
}

/// The keys of the objects `kind` owns that `filter` selects.
pub(crate) fn owned_object_ranges(
    kind: OwnerKind,
    filter: Option<&OwnedObjectFilter>,
) -> Result<KeyRanges, EncodeError> {
    let owned = KeyRanges::prefix(kind.encode()?);
    let Some(filter) = filter else {
        return Ok(owned);
    };
    dnf_ranges(owned, filter, |pattern| {
        Ok(KeyRanges::prefix(
            (kind, TypeFilter::from(pattern)).encode()?,
        ))
    })
}

/// The keys in `object_by_type` that `filter` selects (every live Move struct object when
/// `filter` is `None`).
pub(crate) fn object_by_type_ranges(
    filter: Option<&OwnedObjectFilter>,
) -> Result<KeyRanges, EncodeError> {
    let all = KeyRanges::all();
    let Some(filter) = filter else {
        return Ok(all);
    };
    dnf_ranges(all, filter, |pattern| {
        Ok(KeyRanges::prefix(TypeFilter::from(pattern).encode()?))
    })
}

/// The keys of the dynamic fields `kind` owns that `filter` selects.
///
/// A dynamic field `Field<Name, Value>` is indexed under its full type, and a
/// dynamic object field under `Field<Wrapper<Name>, ID>`. So a name type
/// selects the `Field<Name` prefix, minus every `Field<Wrapper` key (`Wrapper`
/// is itself a type at `0x2`), plus the `Field<Wrapper<Name` prefix.
///
/// A filter selects only `Field` objects. An object can own others: the
/// `Field` object of a dynamic object field owns the field's value object,
/// which only an unfiltered listing returns.
pub(crate) fn dynamic_field_ranges(
    kind: OwnerKind,
    filter: Option<&DynamicFieldFilter>,
) -> Result<KeyRanges, EncodeError> {
    let Some(filter) = filter else {
        return Ok(KeyRanges::prefix(kind.encode()?));
    };

    // `kind || Field` type up to its first type parameter, the name type.
    let mut field = (kind, TypeFilter::Type(field_struct_tag())).encode()?;
    field.push(2); // `Field` has two type parameters.
    // `field || Wrapper` type up to its type-parameter count.
    let mut wrapper = field.clone();
    wrapper.push(TYPE_TAG_STRUCT_VARIANT);
    TypeFilter::Type(wrapper_struct_tag()).encode_into(&mut wrapper)?;
    let dynamic_object_fields = KeyRanges::prefix(wrapper.clone());

    dnf_ranges(
        KeyRanges::prefix(field.clone()),
        filter,
        |predicate| match predicate {
            DynamicFieldPredicate::DynamicObjectField => Ok(dynamic_object_fields.clone()),
            DynamicFieldPredicate::NameType(pattern) => {
                let name = name_type_prefix(pattern)?;
                let inline_value = KeyRanges::prefix([field.as_slice(), &name].concat())
                    .difference(&dynamic_object_fields);
                // `Wrapper` has one type parameter, the name type.
                let object_value = KeyRanges::prefix([wrapper.as_slice(), &[1], &name].concat());
                Ok(inline_value.union(object_value))
            }
        },
    )
}

/// The union over terms of the intersection over literals, each literal's
/// keys taken within `universe`.
fn dnf_ranges<P>(
    universe: KeyRanges,
    filter: &DnfFilter<P>,
    predicate_ranges: impl Fn(&P) -> Result<KeyRanges, EncodeError>,
) -> Result<KeyRanges, EncodeError> {
    let mut selected = KeyRanges::empty();
    for term in &filter.terms {
        let mut term_ranges = universe.clone();
        for literal in term {
            let ranges = predicate_ranges(&literal.predicate)?;
            term_ranges = if literal.negated {
                term_ranges.difference(&ranges)
            } else {
                term_ranges.intersection(&ranges)
            };
        }
        selected = selected.union(term_ranges);
    }
    Ok(selected)
}

/// The leading bytes of the BCS encoding of every type tag `pattern` matches.
fn name_type_prefix(pattern: &NameTypePattern) -> Result<Vec<u8>, EncodeError> {
    match pattern {
        NameTypePattern::Struct(pattern) => {
            let mut prefix = vec![TYPE_TAG_STRUCT_VARIANT];
            TypeFilter::from(pattern).encode_into(&mut prefix)?;
            Ok(prefix)
        }
        NameTypePattern::Exact(name_type) => {
            bcs::to_bytes(name_type).map_err(|e| EncodeError::with_source("bcs encode TypeTag", e))
        }
    }
}

fn field_struct_tag() -> StructTag {
    StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: DYNAMIC_FIELD_MODULE_NAME.to_owned(),
        name: DYNAMIC_FIELD_FIELD_STRUCT_NAME.to_owned(),
        type_params: vec![],
    }
}

fn wrapper_struct_tag() -> StructTag {
    StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: DYNAMIC_OBJECT_FIELD_MODULE_NAME.to_owned(),
        name: DYNAMIC_OBJECT_FIELD_WRAPPER_STRUCT_NAME.to_owned(),
        type_params: vec![],
    }
}

enum ScanRows<'d, K, V> {
    Fwd(Iter<'d, K, V>),
    Rev(RevIter<'d, K, V>),
}

/// The rows of a column family whose keys lie in a [`KeyRanges`], reached by seeking over the
/// gaps between ranges in either forward or reverse key order.
pub(crate) struct FilteredScan<'d, K, V> {
    /// `None` when no range lies within the requested bounds.
    rows: Option<ScanRows<'d, K, V>>,
    ranges: Vec<KeyRange>,
    /// The index of the current range in `ranges` (increasing forward, decreasing in reverse).
    current: usize,
}

fn encode_bound<K: Encode>(bound: Bound<&K>) -> Result<Bound<Vec<u8>>, EncodeError> {
    match bound {
        Bound::Unbounded => Ok(Bound::Unbounded),
        Bound::Included(k) => Ok(Bound::Included(k.encode()?)),
        Bound::Excluded(k) => Ok(Bound::Excluded(k.encode()?)),
    }
}

impl<'d, K, V> FilteredScan<'d, K, V> {
    /// Scans the rows of `map` in `ranges` intersected with `(lower, upper)`, in ascending key
    /// order or reverse when `descending` is set.
    pub(crate) fn open<R: Reader>(
        map: &'d DbMap<K, V, R>,
        ranges: KeyRanges,
        lower: Bound<&K>,
        upper: Bound<&K>,
        descending: bool,
    ) -> Result<Self, Error>
    where
        K: Encode + Decode,
        V: Decode,
    {
        let lower_bytes = encode_bound(lower)?;
        let upper_bytes = encode_bound(upper)?;
        let ranges = ranges
            .restrict(
                lower_bytes.as_ref().map(Vec::as_slice),
                upper_bytes.as_ref().map(Vec::as_slice),
            )
            .0;
        let (Some(first), Some(last)) = (ranges.first(), ranges.last()) else {
            return Ok(Self::empty());
        };
        let iter_lower = if first.start.is_empty() {
            Bound::Unbounded
        } else {
            Bound::Included(first.start.clone())
        };
        let iter_upper = last.end.clone().map_or(Bound::Unbounded, Bound::Excluded);
        let (rows, current) = if descending {
            let rev = map.iter_rev_with::<Vec<u8>>((iter_lower, iter_upper))?;
            (ScanRows::Rev(rev), ranges.len() - 1)
        } else {
            let fwd = map.iter_with::<Vec<u8>>((iter_lower, iter_upper))?;
            (ScanRows::Fwd(fwd), 0)
        };
        Ok(Self {
            rows: Some(rows),
            ranges,
            current,
        })
    }

    fn empty() -> Self {
        Self {
            rows: None,
            ranges: Vec::new(),
            current: 0,
        }
    }
}

impl<K, V> Iterator for FilteredScan<'_, K, V>
where
    K: Decode,
    V: Decode,
{
    type Item = Result<(K, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.rows.as_mut()? {
            ScanRows::Fwd(rows) => loop {
                let Some(key) = rows.raw_key() else {
                    // Exhausted or failed; `next` reports which.
                    return rows.next();
                };
                while self
                    .ranges
                    .get(self.current)
                    .is_some_and(|range| !before_end(key, &range.end))
                {
                    self.current += 1;
                }
                let range = self.ranges.get(self.current)?;
                if range.start.as_slice() <= key {
                    return rows.next();
                }
                rows.seek(&range.start);
            },
            ScanRows::Rev(rows) => loop {
                let Some(key) = rows.raw_key() else {
                    // Exhausted or failed; `next` reports which.
                    return rows.next();
                };
                while key < self.ranges[self.current].start.as_slice() {
                    if self.current == 0 {
                        return None;
                    }
                    self.current -= 1;
                }
                let range = &self.ranges[self.current];
                if before_end(key, &range.end) {
                    return rows.next();
                }
                let end = range
                    .end
                    .as_deref()
                    .expect("before_end is true when end is None");
                rows.skip_past(end);
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use move_core_types::language_storage::TypeTag;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::dynamic_field::DynamicFieldInfo;
    use sui_types::storage::FilterLiteral;
    use sui_types::storage::StructTypePattern;

    use crate::schema::object_by_owner::Key;
    use super::*;

    fn type_tag(s: &str) -> TypeTag {
        sui_types::parse_sui_type_tag(s).unwrap()
    }

    fn struct_tag(s: &str) -> StructTag {
        sui_types::parse_sui_struct_tag(s).unwrap()
    }

    fn dynamic_field(name: &str) -> StructTag {
        DynamicFieldInfo::dynamic_field_type(type_tag(name), type_tag("vector<u8>"))
    }

    fn dynamic_object_field(name: &str) -> StructTag {
        DynamicFieldInfo::dynamic_field_type(
            TypeTag::Struct(Box::new(DynamicFieldInfo::dynamic_object_field_wrapper(
                type_tag(name),
            ))),
            type_tag("0x2::object::ID"),
        )
    }

    fn literal<P>(negated: bool, predicate: P) -> FilterLiteral<P> {
        FilterLiteral { negated, predicate }
    }

    fn key(kind: OwnerKind, type_: StructTag, id: u8) -> Vec<u8> {
        Key {
            kind,
            type_,
            inverted_balance: None,
            object_id: ObjectID::from_single_byte(id),
        }
        .encode()
        .unwrap()
    }

    // Reference semantics of the filters, defined on Move types rather than
    // key bytes. The compiled ranges must select exactly what these accept.

    fn struct_pattern_matches(pattern: &StructTypePattern, tag: &StructTag) -> bool {
        match pattern {
            StructTypePattern::Package(package) => tag.address == *package,
            StructTypePattern::Module { package, module } => {
                tag.address == *package && tag.module == *module
            }
            StructTypePattern::Type(pattern) if pattern.type_params.is_empty() => {
                tag.address == pattern.address
                    && tag.module == pattern.module
                    && tag.name == pattern.name
            }
            StructTypePattern::Type(pattern) => tag == pattern,
        }
    }

    fn name_pattern_matches(pattern: &NameTypePattern, name_type: &TypeTag) -> bool {
        match (pattern, name_type) {
            (NameTypePattern::Struct(pattern), TypeTag::Struct(tag)) => {
                struct_pattern_matches(pattern, tag)
            }
            (NameTypePattern::Struct(_), _) => false,
            (NameTypePattern::Exact(pattern), name_type) => pattern == name_type,
        }
    }

    fn dnf_matches<P>(
        filter: &DnfFilter<P>,
        mut predicate_matches: impl FnMut(&P) -> bool,
    ) -> bool {
        filter.terms.iter().any(|term| {
            term.iter()
                .all(|literal| predicate_matches(&literal.predicate) != literal.negated)
        })
    }

    fn object_filter_matches(filter: &OwnedObjectFilter, object_type: &StructTag) -> bool {
        dnf_matches(filter, |pattern| {
            struct_pattern_matches(pattern, object_type)
        })
    }

    /// Only `Field<Name, Value>` objects can match.
    fn dynamic_field_filter_matches(filter: &DynamicFieldFilter, field_type: &StructTag) -> bool {
        if !DynamicFieldInfo::is_dynamic_field(field_type) {
            return false;
        }
        let [name_type, _value_type] = field_type.type_params.as_slice() else {
            return false;
        };
        // `name_type` is `None` only for a malformed `Wrapper` without exactly
        // one type parameter.
        let (name_type, is_dynamic_object_field) = match name_type {
            TypeTag::Struct(wrapper)
                if DynamicFieldInfo::is_dynamic_object_field_wrapper(wrapper) =>
            {
                match wrapper.type_params.as_slice() {
                    [name_type] => (Some(name_type), true),
                    _ => (None, true),
                }
            }
            name_type => (Some(name_type), false),
        };
        dnf_matches(filter, |predicate| match predicate {
            DynamicFieldPredicate::NameType(pattern) => {
                name_type.is_some_and(|name| name_pattern_matches(pattern, name))
            }
            DynamicFieldPredicate::DynamicObjectField => is_dynamic_object_field,
        })
    }

    #[test]
    fn type_without_params_matches_every_instantiation() {
        let any_coin: StructTypePattern = "0x2::coin::Coin".parse().unwrap();
        let sui_coin: StructTypePattern = "0x2::coin::Coin<0x2::sui::SUI>".parse().unwrap();
        let usdc = struct_tag("0x2::coin::Coin<0xa::usdc::USDC>");
        assert!(struct_pattern_matches(&any_coin, &usdc));
        assert!(!struct_pattern_matches(&sui_coin, &usdc));
        assert!(!struct_pattern_matches(
            &any_coin,
            &struct_tag("0x2::coin::CoinMetadata<0x2::sui::SUI>")
        ));
    }

    #[test]
    fn name_type_matches_the_unwrapped_name_of_dynamic_object_fields() {
        let only = |negated, predicate| DynamicFieldFilter {
            terms: vec![vec![literal(negated, predicate)]],
        };
        let item = only(
            false,
            DynamicFieldPredicate::NameType("0x2::kiosk::Item".parse().unwrap()),
        );
        assert!(dynamic_field_filter_matches(
            &item,
            &dynamic_field("0x2::kiosk::Item")
        ));
        assert!(dynamic_field_filter_matches(
            &item,
            &dynamic_object_field("0x2::kiosk::Item")
        ));

        // The wrapper is defined at 0x2, but the name type is not.
        let framework = only(
            false,
            DynamicFieldPredicate::NameType("0x2".parse().unwrap()),
        );
        assert!(!dynamic_field_filter_matches(
            &framework,
            &dynamic_object_field("0xabc::m::Key")
        ));
        assert!(dynamic_field_filter_matches(
            &framework,
            &dynamic_object_field("0x2::kiosk::Item")
        ));

        let is_object = only(false, DynamicFieldPredicate::DynamicObjectField);
        assert!(dynamic_field_filter_matches(
            &is_object,
            &dynamic_object_field("u64")
        ));
        assert!(!dynamic_field_filter_matches(
            &is_object,
            &dynamic_field("u64")
        ));

        // Only `Field` objects match, even through a negated literal: the
        // value object of a dynamic object field is owned by its `Field`.
        let inline_value = only(true, DynamicFieldPredicate::DynamicObjectField);
        assert!(dynamic_field_filter_matches(
            &inline_value,
            &dynamic_field("u64")
        ));
        assert!(!dynamic_field_filter_matches(
            &inline_value,
            &struct_tag("0x2::coin::Coin<0x2::sui::SUI>")
        ));
    }

    #[test]
    fn dnf_filter_ors_terms_and_ands_literals() {
        let not = |s: &str| literal(true, s.parse::<StructTypePattern>().unwrap());
        // Everything except coins and SuiNS registrations.
        let filter = OwnedObjectFilter {
            terms: vec![vec![
                not("0x2::coin::Coin"),
                not("0xd22b::suins_registration"),
            ]],
        };
        assert!(!object_filter_matches(
            &filter,
            &struct_tag("0x2::coin::Coin<0x2::sui::SUI>")
        ));
        assert!(!object_filter_matches(
            &filter,
            &struct_tag("0xd22b::suins_registration::SuinsRegistration")
        ));
        assert!(object_filter_matches(
            &filter,
            &struct_tag("0x2::kiosk::KioskOwnerCap")
        ));
        assert!(!object_filter_matches(
            &OwnedObjectFilter { terms: vec![] },
            &struct_tag("0x2::a::B")
        ));
    }

    #[test]
    fn struct_type_tag_variant_matches_bcs() {
        let tag = TypeTag::Struct(Box::new(struct_tag("0x2::sui::SUI")));
        assert_eq!(bcs::to_bytes(&tag).unwrap()[0], TYPE_TAG_STRUCT_VARIANT);
    }

    #[test]
    fn range_algebra() {
        let a = KeyRanges::prefix(vec![1]);
        let b = KeyRanges::prefix(vec![1, 5]);
        let not_b = a.difference(&b);
        assert_eq!(
            not_b.0,
            vec![
                KeyRange {
                    start: vec![1],
                    end: Some(vec![1, 5]),
                },
                KeyRange {
                    start: vec![1, 6],
                    end: Some(vec![2]),
                },
            ]
        );
        assert_eq!(not_b.clone().union(b.clone()), a);
        assert_eq!(not_b.intersection(&b), KeyRanges::empty());
        assert_eq!(
            KeyRanges::empty().complement().complement(),
            KeyRanges::empty()
        );
        let top = KeyRanges::prefix(vec![0xFF]);
        assert_eq!(top.0[0].end, None);
        assert_eq!(top.complement().complement(), top);
    }

    /// The compiled ranges hold exactly the keys of `kind` whose type the
    /// reference matchers accept, for every key in a corpus spanning other
    /// owners, every specificity level, and both kinds of dynamic field.
    #[test]
    fn ranges_select_exactly_the_matching_keys() {
        let owner = OwnerKind::AddressOwner(SuiAddress::from(ObjectID::from_single_byte(0x10)));
        let parent = OwnerKind::ObjectOwner(SuiAddress::from(ObjectID::from_single_byte(0x10)));
        let neighbours = [
            OwnerKind::AddressOwner(SuiAddress::from(ObjectID::from_single_byte(0x11))),
            OwnerKind::ObjectOwner(SuiAddress::from(ObjectID::from_single_byte(0x0f))),
        ];

        let object_types: Vec<StructTag> = [
            "0x2::coin::Coin<0x2::sui::SUI>",
            "0x2::coin::Coin<0xa::usdc::USDC>",
            "0x2::coin::CoinMetadata<0x2::sui::SUI>",
            "0x2::kiosk::KioskOwnerCap",
            "0x2::kiosk::Kiosk",
            "0xd22b::suins_registration::SuinsRegistration",
            "0xabc::m::Thing<0x2::coin::Coin<0x2::sui::SUI>>",
            "0xff::m::T",
        ]
        .into_iter()
        .map(struct_tag)
        .collect();
        let field_types: Vec<StructTag> = [
            "u64",
            "address",
            "vector<u8>",
            "0x2::kiosk::Item",
            "0x2::kiosk::Listing",
            "0x2::dynamic_object_field::Wrapper<u64>",
            "0xabc::m::Key",
            "0xabc::m::Key<u8>",
        ]
        .into_iter()
        .flat_map(|name| [dynamic_field(name), dynamic_object_field(name)])
        .chain(object_types.iter().cloned())
        .collect();

        let object_filters: Vec<OwnedObjectFilter> = vec![
            OwnedObjectFilter::object_type(struct_tag("0x2::coin::Coin")),
            OwnedObjectFilter::object_type(struct_tag("0x2::coin::Coin<0x2::sui::SUI>")),
            OwnedObjectFilter {
                terms: vec![vec![
                    literal(true, "0x2::coin::Coin".parse().unwrap()),
                    literal(true, "0xd22b::suins_registration".parse().unwrap()),
                ]],
            },
            OwnedObjectFilter {
                terms: vec![
                    vec![
                        literal(false, "0x2".parse().unwrap()),
                        literal(true, "0x2::kiosk".parse().unwrap()),
                    ],
                    vec![literal(false, "0x2::kiosk::Kiosk".parse().unwrap())],
                    vec![literal(true, "0xabc".parse::<StructTypePattern>().unwrap())],
                ],
            },
            OwnedObjectFilter { terms: vec![] },
            OwnedObjectFilter {
                terms: vec![vec![]],
            },
        ];
        for filter in &object_filters {
            let ranges = owned_object_ranges(owner, Some(filter)).unwrap();
            for type_ in &object_types {
                assert_eq!(
                    ranges.contains(&key(owner, type_.clone(), 1)),
                    object_filter_matches(filter, type_),
                    "{filter:?} on {type_}",
                );
                for neighbour in neighbours {
                    assert!(!ranges.contains(&key(neighbour, type_.clone(), 1)));
                }
            }
        }

        let name = |s: &str| DynamicFieldPredicate::NameType(s.parse().unwrap());
        let field_filters: Vec<DynamicFieldFilter> = vec![
            DynamicFieldFilter {
                terms: vec![vec![literal(false, name("0x2::kiosk::Item"))]],
            },
            // `0x2` is a prefix of the `Wrapper` type of every dynamic object field.
            DynamicFieldFilter {
                terms: vec![vec![literal(false, name("0x2"))]],
            },
            DynamicFieldFilter {
                terms: vec![vec![literal(false, name("0x2::dynamic_object_field"))]],
            },
            DynamicFieldFilter {
                terms: vec![vec![literal(true, name("0x2"))]],
            },
            DynamicFieldFilter {
                terms: vec![vec![literal(false, name("u64"))]],
            },
            DynamicFieldFilter {
                terms: vec![vec![literal(false, name("0xabc::m::Key"))]],
            },
            DynamicFieldFilter {
                terms: vec![vec![
                    literal(false, DynamicFieldPredicate::DynamicObjectField),
                    literal(true, name("0x2::kiosk")),
                ]],
            },
            DynamicFieldFilter {
                terms: vec![
                    vec![literal(true, DynamicFieldPredicate::DynamicObjectField)],
                    vec![literal(false, name("0x2::kiosk::Item"))],
                ],
            },
        ];
        for filter in &field_filters {
            let ranges = dynamic_field_ranges(parent, Some(filter)).unwrap();
            for type_ in &field_types {
                assert_eq!(
                    ranges.contains(&key(parent, type_.clone(), 1)),
                    dynamic_field_filter_matches(filter, type_),
                    "{filter:?} on {type_}",
                );
                for neighbour in neighbours {
                    assert!(!ranges.contains(&key(neighbour, type_.clone(), 1)));
                }
            }
        }
    }
}
