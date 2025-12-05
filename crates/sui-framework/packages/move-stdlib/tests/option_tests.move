// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::option_tests;

use std::unit_test::assert_eq;

#[test]
fun option_none_is_none() {
    let none = option::none<u64>();
    assert!(none.is_none());
    assert!(!none.is_some());
}

#[test]
fun option_some_is_some() {
    let some = option::some(5u64);
    assert!(!some.is_none());
    assert!(some.is_some());
}

#[test]
fun option_contains() {
    let none = option::none<u64>();
    let some = option::some(5u64);
    let some_other = option::some(6u64);
    assert!(some.contains(&5));
    assert!(some_other.contains(&6));
    assert!(!none.contains(&5));
    assert!(!some_other.contains(&5));
}

#[test]
fun option_borrow_some() {
    let some = option::some(5);
    let some_other = option::some(6u64);
    assert_eq!(*some.borrow(), 5u64);
    assert_eq!(*some_other.borrow(), 6);
}

#[test, expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun option_borrow_none() {
    option::none<u64>().borrow();
}

#[test]
fun borrow_mut_some() {
    let mut some = option::some(1);
    let ref = some.borrow_mut();
    *ref = 10;
    assert_eq!(*some.borrow(), 10u64);
}

#[test, expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun borrow_mut_none() {
    option::none<u64>().borrow_mut();
}

#[test]
fun borrow_with_default() {
    let none = option::none<u64>();
    let some = option::some(5u64);
    assert_eq!(*some.borrow_with_default(&7), 5);
    assert_eq!(*none.borrow_with_default(&7), 7);
}

#[test]
fun get_with_default() {
    let none = option::none<u64>();
    let some = option::some(5u64);
    assert_eq!(option::get_with_default(&some, 7), 5);
    assert_eq!(option::get_with_default(&none, 7), 7);
}

#[test]
fun extract_some() {
    let mut opt = option::some(1u64);
    assert_eq!(opt.extract(), 1);
    assert!(opt.is_none());
}

#[test, expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun extract_none() {
    option::none<u64>().extract();
}

#[test]
fun swap_some() {
    let mut some = option::some(5u64);
    assert_eq!(some.swap(1), 5);
    assert_eq!(*some.borrow(), 1);
}

#[test]
fun swap_or_fill_some() {
    let mut some = option::some(5u64);
    assert_eq!(some.swap_or_fill(1), option::some(5));
    assert_eq!(*some.borrow(), 1);
}

#[test]
fun swap_or_fill_none() {
    let mut none = option::none();
    assert_eq!(none.swap_or_fill(1u64), option::none());
    assert_eq!(*none.borrow(), 1);
}

#[test, expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun swap_none() {
    option::none<u64>().swap(1);
}

#[test]
fun fill_none() {
    let mut none = option::none<u64>();
    none.fill(3);
    assert!(none.is_some());
    assert_eq!(*none.borrow(), 3);
}

#[test, expected_failure(abort_code = option::EOPTION_IS_SET)]
fun fill_some() {
    option::some(3u64).fill(0);
}

#[test]
fun destroy_with_default() {
    assert_eq!(option::none<u64>().destroy_with_default(4), 4);
    assert_eq!(option::some(4u64).destroy_with_default(5), 4);
}

#[test]
fun destroy_some() {
    assert_eq!(option::some(4u64).destroy_some(), 4);
}

#[test, expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun destroy_some_none() {
    option::none<u64>().destroy_some();
}

#[test]
fun destroy_none() {
    option::none<u64>().destroy_none();
}

#[test, expected_failure(abort_code = option::EOPTION_IS_SET)]
fun destroy_none_some() {
    option::some<u64>(0).destroy_none();
}

#[test]
fun into_vec_some() {
    let mut v = option::some<u64>(0).to_vec();
    assert_eq!(v.length(), 1);
    let x = v.pop_back();
    assert_eq!(x, 0);
}

#[test]
fun into_vec_none() {
    let v: vector<u64> = option::none().to_vec();
    assert!(v.is_empty());
}

// === Macros ===

public struct NoDrop {}

#[test]
fun do_destroy() {
    let mut counter = 0;
    option::some(5u64).destroy!(|x| counter = x);
    option::some(10).do!(|x| counter = counter + x);

    assert_eq!(counter, 15);

    let some = option::some(NoDrop {});
    let none = option::none<NoDrop>();

    some.do!(|el| { let NoDrop {} = el; });
    none.do!(|el| { let NoDrop {} = el; });

    option::some(5u64).do!(|x| x); // return value
    option::some(5u64).do!(|_| {}); // no return value

    option::some(5u64).destroy!(|x| x); // return value
    option::some(5u64).destroy!(|_| {}); // no return value
}

#[test]
fun do_ref_mut() {
    let mut counter = 0u64;
    let mut opt = option::some(5);
    opt.do_mut!(|x| *x = 100);
    opt.do_ref!(|x| counter = *x);

    assert_eq!(counter, 100);

    opt.do_ref!(|x| *x); // return value
    opt.do_ref!(|_| {}); // no return value
    opt.do_mut!(|_| 5u64); // return value
    opt.do_mut!(|_| {}); // no return value
}

#[test]
fun map_map_ref() {
    assert_eq!(option::some(5u64).map!(|x| vector[x]), option::some(vector[5]));
    assert_eq!(option::some(5u64).map_ref!(|x| vector[*x]), option::some(vector[5]));
    assert_eq!(option::none<u8>().map!(|x| vector[x]), option::none());
    assert_eq!(option::none<u8>().map_ref!(|x| vector[*x]), option::none());
}

#[test]
fun map_no_drop() {
    let none = option::none<NoDrop>().map!(|el| {
        let NoDrop {} = el;
        100u64
    });
    let some = option::some(NoDrop {}).map!(|el| {
        let NoDrop {} = el;
        100u64
    });

    assert_eq!(none, option::none());
    assert_eq!(some, option::some(100));
}

#[test]
fun or_no_drop() {
    let none = option::none<NoDrop>().or!(option::some(NoDrop {}));
    let some = option::some(NoDrop {}).or!(option::some(NoDrop {}));

    assert!(none.is_some());
    assert!(some.is_some());

    let NoDrop {} = none.destroy_some();
    let NoDrop {} = some.destroy_some();
}

#[test]
fun and_no_drop() {
    let none = option::none<NoDrop>().and!(|e| {
        let NoDrop {} = e;
        option::some(100u64)
    });

    let some = option::some(NoDrop {}).and!(|e| {
        let NoDrop {} = e;
        option::some(100u64)
    });

    assert_eq!(some, option::some(100));
    assert_eq!(none, option::none());
}

#[test]
fun filter() {
    assert!(option::some(5u64).filter!(|x| *x == 5) == option::some(5));
    assert!(option::some(5u64).filter!(|x| *x == 6) == option::none());
}

#[test]
fun is_some_and() {
    assert!(option::some(5u64).is_some_and!(|x| *x == 5));
    assert!(!option::some(5u64).is_some_and!(|x| *x == 6));
    assert!(!option::none().is_some_and!(|x| *x == 5u64));
}

#[test]
fun destroy_or() {
    assert_eq!(option::none().destroy_or!(10u64), 10);
    assert_eq!(option::some(5u64).destroy_or!(10), 5);

    let some = option::some(10u64);
    assert_eq!(some.destroy_or!(0), 10);
    assert!(some.is_some()); // value was copied!
}

#[test]
fun destroy_or_no_drop() {
    let none = option::none<NoDrop>().destroy_or!(NoDrop {});
    let some = option::some(NoDrop {}).destroy_or!(abort);

    let NoDrop {} = some;
    let NoDrop {} = none;
}

#[test]
fun extract_or() {
    let mut none = option::none<u64>();
    assert_eq!(none.extract_or!(10), 10);
    assert!(none.is_none());

    let mut some = option::some(5);
    assert_eq!(some.extract_or!(10), 5u64);
    assert!(some.is_none());
}
