// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use move_core_types::account_address::AccountAddress;
use move_package::{
    resolution::resolving_table::ResolvingTable,
    source_package::parsed_manifest::{NamedAddress, PackageName},
};

#[test]
fn definitions() {
    let mut table = ResolvingTable::new();

    let p0 = PackageName::from("p0");
    let n0 = NamedAddress::from("n0");
    let n1 = NamedAddress::from("n1");
    let a0 = AccountAddress::random();
    let a1 = AccountAddress::random();
    assert_ne!(a0, a1);

    table.define((p0, n0), Some(a0)).expect("Definition");
    table.define((p0, n1), None).expect("Declaration");
    table.define((p0, n0), Some(a0)).expect("Redefinition");
    table.define((p0, n1), Some(a1)).expect("Prev declaration");

    assert!(
        table.define((p0, n0), Some(a1)).is_err(),
        "Conflicting definition",
    );

    assert_eq!(Some(&a0), table.get((p0, n0)));
    assert_eq!(Some(&a1), table.get((p0, n1)));
}

#[test]
fn unification() {
    let mut table = ResolvingTable::new();

    let p0 = PackageName::from("p0");
    let n0 = NamedAddress::from("n0");
    let n1 = NamedAddress::from("n1");
    let n2 = NamedAddress::from("n2");
    let n3 = NamedAddress::from("n3");
    let n4 = NamedAddress::from("n4");
    let a0 = AccountAddress::random();
    let a1 = AccountAddress::random();
    assert_ne!(a0, a1);

    table.define((p0, n0), Some(a0)).expect("Definition");
    table.unify((p0, n0), (p0, n1)).expect("Unify fresh");

    assert_eq!(Some(&a0), table.get((p0, n0)));
    assert_eq!(Some(&a0), table.get((p0, n1)));

    table.define((p0, n2), None).expect("Declaration");
    table.unify((p0, n2), (p0, n3)).expect("Unify decl. 1");
    table.unify((p0, n3), (p0, n4)).expect("Unify decl. 2");
    table.define((p0, n4), Some(a1)).expect("Assign to chain");

    assert_eq!(Some(&a1), table.get((p0, n2)));
    assert_eq!(Some(&a1), table.get((p0, n3)));
    assert_eq!(Some(&a1), table.get((p0, n4)));

    table
        .unify((p0, n2), (p0, n3))
        .expect("Unify already unified");

    assert!(
        table.unify((p0, n3), (p0, n1)).is_err(),
        "Conflicting definitions either side of unification",
    );
}

#[test]
fn bindings() {
    let mut table = ResolvingTable::new();

    let p0 = PackageName::from("p0");
    let p1 = PackageName::from("p1");
    let n0 = NamedAddress::from("n0");
    let n1 = NamedAddress::from("n1");
    let n2 = NamedAddress::from("n2");
    let a0 = AccountAddress::random();
    let a1 = AccountAddress::random();

    table.define((p0, n0), Some(a0)).unwrap();
    table.define((p0, n1), None).unwrap();
    table.define((p1, n2), Some(a1)).unwrap();

    assert_eq!(
        table.bindings(p0).collect::<HashSet<_>>(),
        HashSet::from([(n0, &Some(a0)), (n1, &None)]),
        "Bindings include unassigned addresses",
    );

    assert_eq!(
        table.bindings(p1).collect::<HashSet<_>>(),
        HashSet::from([(n2, &Some(a1))]),
    );

    table.unify((p0, n1), (p1, n2)).unwrap();
    table.unify((p0, n0), (p1, n0)).unwrap();

    assert_eq!(
        table.bindings(p0).collect::<HashSet<_>>(),
        HashSet::from([(n0, &Some(a0)), (n1, &Some(a1))]),
        "Bindings updated post unification",
    );

    assert_eq!(
        table.bindings(p1).collect::<HashSet<_>>(),
        HashSet::from([(n2, &Some(a1)), (n0, &Some(a0))]),
        "Bindings updated post unification",
    );
}

#[test]
fn contains() {
    let mut table = ResolvingTable::new();

    let p0 = PackageName::from("p0");
    let p1 = PackageName::from("p1");
    let n0 = NamedAddress::from("n0");
    let n1 = NamedAddress::from("n1");
    let a0 = AccountAddress::random();

    table.define((p0, p1), Some(a0)).unwrap();
    table.define((p0, n0), None).unwrap();

    // An assignment with a binding counts as contained.
    assert!(table.contains((p0, p1)));

    // So does an assignment without a binding.
    assert!(table.contains((p0, n0)));

    // But not a completely fresh address.
    assert!(!table.contains((p0, n1)));
}
