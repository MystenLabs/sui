// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

#[test]
fn test_from() {
    let _ = Symbol::from("this shouldn't panic");
}

#[test]
fn test_as_str() {
    let s = Symbol::from("a");
    assert_eq!(s.as_str(), "a");
}

#[test]
fn test_as_str_lifetime_is_static() {
    let symbol = Symbol::from("1");
    let string = symbol.as_str();

    // Grow the symbol pool with nearly 100 new symbols.
    for n in 2..100 {
        let _symbol_n = Symbol::from(n.to_string());
    }

    // Both the symbol and its reference to string data ought to remain valid,
    // no matter how many elements were added to the underlying pool.
    assert_eq!(symbol.as_str(), "1");
    assert_eq!(string, "1");
}

#[test]
fn test_display() {
    assert_eq!(format!("{}", Symbol::from("hola")), "hola");
}

#[test]
fn test_ord() {
    assert!(Symbol::from("aardvark") < Symbol::from("bear"));
    assert!(Symbol::from("bear") <= Symbol::from("bear"));
    assert!(Symbol::from("cat") > Symbol::from("bear"));
    assert!(Symbol::from("dog") >= Symbol::from("cat"));
}
