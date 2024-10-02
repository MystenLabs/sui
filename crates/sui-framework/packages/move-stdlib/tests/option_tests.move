// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::option_tests;

#[test]
fun option_none_is_none() {
    let none = option::none<u64>();
    assert!(none.is_none());
    assert!(!none.is_some());
}

#[test]
fun option_some_is_some() {
    let some = option::some(5);
    assert!(!some.is_none());
    assert!(some.is_some());
}

#[test]
fun option_contains() {
    let none = option::none<u64>();
    let some = option::some(5);
    let some_other = option::some(6);
    assert!(some.contains(&5));
    assert!(some_other.contains(&6));
    assert!(!none.contains(&5));
    assert!(!some_other.contains(&5));
}

#[test]
fun option_borrow_some() {
    let some = option::some(5);
    let some_other = option::some(6);
    assert!(*some.borrow() == 5);
    assert!(*some_other.borrow() == 6);
}

#[test]
#[expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun option_borrow_none() {
    option::none<u64>().borrow();
}

#[test]
fun borrow_mut_some() {
    let mut some = option::some(1);
    let ref = some.borrow_mut();
    *ref = 10;
    assert!(*some.borrow() == 10);
}

#[test]
#[expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun borrow_mut_none() {
    option::none<u64>().borrow_mut();
}

#[test]
fun borrow_with_default() {
    let none = option::none<u64>();
    let some = option::some(5);
    assert!(*some.borrow_with_default(&7) == 5);
    assert!(*none.borrow_with_default(&7) == 7);
}

#[test]
fun get_with_default() {
    let none = option::none<u64>();
    let some = option::some(5);
    assert!(option::get_with_default(&some, 7) == 5);
    assert!(option::get_with_default(&none, 7) == 7);
}

#[test]
fun extract_some() {
    let mut opt = option::some(1);
    assert!(opt.extract() == 1);
    assert!(opt.is_none());
}

#[test]
#[expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun extract_none() {
    option::none<u64>().extract();
}

#[test]
fun swap_some() {
    let mut some = option::some(5);
    assert!(some.swap(1) == 5);
    assert!(*some.borrow() == 1);
}

#[test]
fun swap_or_fill_some() {
    let mut some = option::some(5);
    assert!(some.swap_or_fill(1) == option::some(5));
    assert!(*some.borrow() == 1);
}

#[test]
fun swap_or_fill_none() {
    let mut none = option::none();
    assert!(none.swap_or_fill(1) == option::none());
    assert!(*none.borrow() == 1);
}

#[test]
#[expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun swap_none() {
    option::none<u64>().swap(1);
}

#[test]
fun fill_none() {
    let mut none = option::none<u64>();
    none.fill(3);
    assert!(none.is_some());
    assert!(*none.borrow() == 3);
}

#[test]
#[expected_failure(abort_code = option::EOPTION_IS_SET)]
fun fill_some() {
    option::some(3).fill(0);
}

#[test]
fun destroy_with_default() {
    assert!(option::none<u64>().destroy_with_default(4) == 4);
    assert!(option::some(4).destroy_with_default(5) == 4);
}

#[test]
fun destroy_some() {
    assert!(option::some(4).destroy_some() == 4);
}

#[test]
#[expected_failure(abort_code = option::EOPTION_NOT_SET)]
fun destroy_some_none() {
    option::none<u64>().destroy_some();
}

#[test]
fun destroy_none() {
    option::none<u64>().destroy_none();
}

#[test]
#[expected_failure(abort_code = option::EOPTION_IS_SET)]
fun destroy_none_some() {
    option::some<u64>(0).destroy_none();
}

#[test]
fun into_vec_some() {
    let mut v = option::some<u64>(0).to_vec();
    assert!(v.length() == 1);
    let x = v.pop_back();
    assert!(x == 0);
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
    option::some(5).destroy!(|x| counter = x);
    option::some(10).do!(|x| counter = counter + x);

    assert!(counter == 15);

    let some = option::some(NoDrop {});
    let none = option::none<NoDrop>();

    some.do!(|el| { let NoDrop {} = el; });
    none.do!(|el| { let NoDrop {} = el; });
}

#[test]
fun do_ref_mut() {
    let mut counter = 0;
    let mut opt = option::some(5);
    opt.do_mut!(|x| *x = 100);
    opt.do_ref!(|x| counter = *x);

    assert!(counter == 100);
}

#[test]
fun map_map_ref() {
    assert!(option::some(5).map!(|x| vector[x]) == option::some(vector[5]));
    assert!(option::some(5).map_ref!(|x| vector[*x]) == option::some(vector[5]));
    assert!(option::none<u8>().map!(|x| vector[x]) == option::none());
    assert!(option::none<u8>().map_ref!(|x| vector[*x]) == option::none());
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

    assert!(none == option::none());
    assert!(some == option::some(100));
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
        option::some(100)
    });

    let some = option::some(NoDrop {}).and!(|e| {
        let NoDrop {} = e;
        option::some(100)
    });

    assert!(some == option::some(100));
    assert!(none == option::none());
}

#[test]
fun filter() {
    assert!(option::some(5).filter!(|x| *x == 5) == option::some(5));
    assert!(option::some(5).filter!(|x| *x == 6) == option::none());
}

#[test]
fun is_some_and() {
    assert!(option::some(5).is_some_and!(|x| *x == 5));
    assert!(!option::some(5).is_some_and!(|x| *x == 6));
    assert!(!option::none().is_some_and!(|x| *x == 5));
}

#[test]
fun destroy_or() {
    assert!(option::none().destroy_or!(10) == 10);
    assert!(option::some(5).destroy_or!(10) == 5);
}

#[test]
fun destroy_or_no_drop() {
    let none = option::none<NoDrop>().destroy_or!(NoDrop {});
    let some = option::some(NoDrop {}).destroy_or!(NoDrop {});

    let NoDrop {} = some;
    let NoDrop {} = none;
}
