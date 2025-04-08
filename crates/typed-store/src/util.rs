// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bincode::Options;
use serde::Serialize;
use std::ops::{Bound, RangeBounds};

#[inline]
pub fn be_fix_int_ser<S>(t: &S) -> Vec<u8>
where
    S: ?Sized + serde::Serialize,
{
    bincode::DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding()
        .serialize(t)
        .expect("failed to serialize via be_fix_int_ser method")
}

pub(crate) fn iterator_bounds<K>(
    lower_bound: Option<K>,
    upper_bound: Option<K>,
) -> (Option<Vec<u8>>, Option<Vec<u8>>)
where
    K: Serialize,
{
    (
        lower_bound.map(|b| be_fix_int_ser(&b)),
        upper_bound.map(|b| be_fix_int_ser(&b)),
    )
}

pub(crate) fn iterator_bounds_with_range<K>(
    range: impl RangeBounds<K>,
) -> (Option<Vec<u8>>, Option<Vec<u8>>)
where
    K: Serialize,
{
    let iterator_lower_bound = match range.start_bound() {
        Bound::Included(lower_bound) => {
            // Rocksdb lower bound is inclusive by default so nothing to do
            Some(be_fix_int_ser(&lower_bound))
            // readopts.set_iterate_lower_bound(key_buf);
        }
        Bound::Excluded(lower_bound) => {
            let mut key_buf = be_fix_int_ser(&lower_bound);

            // Since we want exclusive, we need to increment the key to exclude the previous
            big_endian_saturating_add_one(&mut key_buf);
            Some(key_buf)
            // readopts.set_iterate_lower_bound(key_buf);
        }
        Bound::Unbounded => None,
    };
    let iterator_upper_bound = match range.end_bound() {
        Bound::Included(upper_bound) => {
            let mut key_buf = be_fix_int_ser(&upper_bound);

            // If the key is already at the limit, there's nowhere else to go, so no upper bound
            if !is_max(&key_buf) {
                // Since we want exclusive, we need to increment the key to get the upper bound
                big_endian_saturating_add_one(&mut key_buf);
                // readopts.set_iterate_upper_bound(key_buf);
            }
            Some(key_buf)
        }
        Bound::Excluded(upper_bound) => {
            // Rocksdb upper bound is inclusive by default so nothing to do
            Some(be_fix_int_ser(&upper_bound))
            // readopts.set_iterate_upper_bound(key_buf);
        }
        Bound::Unbounded => None,
    };
    (iterator_lower_bound, iterator_upper_bound)
}

/// Given a vec<u8>, find the value which is one more than the vector
/// if the vector was a big endian number.
/// If the vector is already minimum, don't change it.
fn big_endian_saturating_add_one(v: &mut [u8]) {
    if is_max(v) {
        return;
    }
    for i in (0..v.len()).rev() {
        if v[i] == u8::MAX {
            v[i] = 0;
        } else {
            v[i] += 1;
            break;
        }
    }
}

/// Check if all the bytes in the vector are 0xFF
fn is_max(v: &[u8]) -> bool {
    v.iter().all(|&x| x == u8::MAX)
}

#[allow(clippy::assign_op_pattern)]
#[test]
fn test_helpers() {
    let v = vec![];
    assert!(is_max(&v));

    fn check_add(v: Vec<u8>) {
        let mut v = v;
        let num = Num32::from_big_endian(&v);
        big_endian_saturating_add_one(&mut v);
        assert!(num + 1 == Num32::from_big_endian(&v));
    }

    uint::construct_uint! {
        // 32 byte number
        struct Num32(4);
    }

    let mut v = vec![255; 32];
    big_endian_saturating_add_one(&mut v);
    assert!(Num32::MAX == Num32::from_big_endian(&v));

    check_add(vec![1; 32]);
    check_add(vec![6; 32]);
    check_add(vec![254; 32]);

    // TBD: More tests coming with randomized arrays
}
