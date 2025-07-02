// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[defines_primitive(vector)]
/// A variable-sized container that can hold any type. Indexing is 0-based, and
/// vectors are growable. This module has many native functions.
module std::vector;

/// Allows calling `.to_string()` on a vector of `u8` to get a utf8 `String`.
public use fun std::string::utf8 as vector.to_string;

/// Allows calling `.try_to_string()` on a vector of `u8` to get a utf8 `String`.
/// This will return `None` if the vector is not valid utf8.
public use fun std::string::try_utf8 as vector.try_to_string;

/// Allows calling `.to_ascii_string()` on a vector of `u8` to get an `ascii::String`.
public use fun std::ascii::string as vector.to_ascii_string;

/// Allows calling `.try_to_ascii_string()` on a vector of `u8` to get an
/// `ascii::String`. This will return `None` if the vector is not valid ascii.
public use fun std::ascii::try_string as vector.try_to_ascii_string;

/// The index into the vector is out of bounds
const EINDEX_OUT_OF_BOUNDS: u64 = 0x20000;

#[bytecode_instruction]
/// Create an empty vector.
public native fun empty<Element>(): vector<Element>;

#[bytecode_instruction]
/// Return the length of the vector.
public native fun length<Element>(v: &vector<Element>): u64;

#[syntax(index)]
#[bytecode_instruction]
/// Acquire an immutable reference to the `i`th element of the vector `v`.
/// Aborts if `i` is out of bounds.
public native fun borrow<Element>(v: &vector<Element>, i: u64): &Element;

#[bytecode_instruction]
/// Add element `e` to the end of the vector `v`.
public native fun push_back<Element>(v: &mut vector<Element>, e: Element);

#[syntax(index)]
#[bytecode_instruction]
/// Return a mutable reference to the `i`th element in the vector `v`.
/// Aborts if `i` is out of bounds.
public native fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;

#[bytecode_instruction]
/// Pop an element from the end of vector `v`.
/// Aborts if `v` is empty.
public native fun pop_back<Element>(v: &mut vector<Element>): Element;

#[bytecode_instruction]
/// Destroy the vector `v`.
/// Aborts if `v` is not empty.
public native fun destroy_empty<Element>(v: vector<Element>);

#[bytecode_instruction]
/// Swaps the elements at the `i`th and `j`th indices in the vector `v`.
/// Aborts if `i` or `j` is out of bounds.
public native fun swap<Element>(v: &mut vector<Element>, i: u64, j: u64);

/// Return an vector of size one containing element `e`.
public fun singleton<Element>(e: Element): vector<Element> {
    let mut v = empty();
    v.push_back(e);
    v
}

/// Reverses the order of the elements in the vector `v` in place.
public fun reverse<Element>(v: &mut vector<Element>) {
    let len = v.length();
    if (len == 0) return;

    let mut front_index = 0;
    let mut back_index = len - 1;
    while (front_index < back_index) {
        v.swap(front_index, back_index);
        front_index = front_index + 1;
        back_index = back_index - 1;
    }
}

/// Pushes all of the elements of the `other` vector into the `lhs` vector.
public fun append<Element>(lhs: &mut vector<Element>, other: vector<Element>) {
    other.do!(|e| lhs.push_back(e));
}

/// Return `true` if the vector `v` has no elements and `false` otherwise.
public fun is_empty<Element>(v: &vector<Element>): bool {
    v.length() == 0
}

/// Return true if `e` is in the vector `v`.
/// Otherwise, returns false.
public fun contains<Element>(v: &vector<Element>, e: &Element): bool {
    let mut i = 0;
    let len = v.length();
    while (i < len) {
        if (&v[i] == e) return true;
        i = i + 1;
    };
    false
}

/// Return `(true, i)` if `e` is in the vector `v` at index `i`.
/// Otherwise, returns `(false, 0)`.
public fun index_of<Element>(v: &vector<Element>, e: &Element): (bool, u64) {
    let mut i = 0;
    let len = v.length();
    while (i < len) {
        if (&v[i] == e) return (true, i);
        i = i + 1;
    };
    (false, 0)
}

/// Remove the `i`th element of the vector `v`, shifting all subsequent elements.
/// This is O(n) and preserves ordering of elements in the vector.
/// Aborts if `i` is out of bounds.
public fun remove<Element>(v: &mut vector<Element>, mut i: u64): Element {
    let mut len = v.length();
    // i out of bounds; abort
    if (i >= len) abort EINDEX_OUT_OF_BOUNDS;

    len = len - 1;
    while (i < len) {
        v.swap(i, { i = i + 1; i });
    };
    v.pop_back()
}

/// Insert `e` at position `i` in the vector `v`.
/// If `i` is in bounds, this shifts the old `v[i]` and all subsequent elements to the right.
/// If `i == v.length()`, this adds `e` to the end of the vector.
/// This is O(n) and preserves ordering of elements in the vector.
/// Aborts if `i > v.length()`
public fun insert<Element>(v: &mut vector<Element>, e: Element, mut i: u64) {
    let len = v.length();
    // i too big abort
    if (i > len) abort EINDEX_OUT_OF_BOUNDS;

    v.push_back(e);
    while (i < len) {
        v.swap(i, len);
        i = i + 1
    }
}

/// Swap the `i`th element of the vector `v` with the last element and then pop the vector.
/// This is O(1), but does not preserve ordering of elements in the vector.
/// Aborts if `i` is out of bounds.
public fun swap_remove<Element>(v: &mut vector<Element>, i: u64): Element {
    assert!(v.length() != 0, EINDEX_OUT_OF_BOUNDS);
    let last_idx = v.length() - 1;
    v.swap(i, last_idx);
    v.pop_back()
}

// === Macros ===

/// Create a vector of length `n` by calling the function `f` on each index.
public macro fun tabulate<$T>($n: u64, $f: |u64| -> $T): vector<$T> {
    let mut v = vector[];
    let n = $n;
    n.do!(|i| v.push_back($f(i)));
    v
}

/// Destroy the vector `v` by calling `f` on each element and then destroying the vector.
/// Does not preserve the order of elements in the vector (starts from the end of the vector).
public macro fun destroy<$T, $R: drop>($v: vector<$T>, $f: |$T| -> $R) {
    let mut v = $v;
    v.length().do!(|_| $f(v.pop_back()));
    v.destroy_empty();
}

/// Destroy the vector `v` by calling `f` on each element and then destroying the vector.
/// Preserves the order of elements in the vector.
public macro fun do<$T, $R: drop>($v: vector<$T>, $f: |$T| -> $R) {
    let mut v = $v;
    v.reverse();
    v.length().do!(|_| $f(v.pop_back()));
    v.destroy_empty();
}

/// Perform an action `f` on each element of the vector `v`. The vector is not modified.
public macro fun do_ref<$T, $R: drop>($v: &vector<$T>, $f: |&$T| -> $R) {
    let v = $v;
    v.length().do!(|i| $f(&v[i]))
}

/// Perform an action `f` on each element of the vector `v`.
/// The function `f` takes a mutable reference to the element.
public macro fun do_mut<$T, $R: drop>($v: &mut vector<$T>, $f: |&mut $T| -> $R) {
    let v = $v;
    v.length().do!(|i| $f(&mut v[i]))
}

/// Map the vector `v` to a new vector by applying the function `f` to each element.
/// Preserves the order of elements in the vector, first is called first.
public macro fun map<$T, $U>($v: vector<$T>, $f: |$T| -> $U): vector<$U> {
    let v = $v;
    let mut r = vector[];
    v.do!(|e| r.push_back($f(e)));
    r
}

/// Map the vector `v` to a new vector by applying the function `f` to each element.
/// Preserves the order of elements in the vector, first is called first.
public macro fun map_ref<$T, $U>($v: &vector<$T>, $f: |&$T| -> $U): vector<$U> {
    let v = $v;
    let mut r = vector[];
    v.do_ref!(|e| r.push_back($f(e)));
    r
}

/// Filter the vector `v` by applying the function `f` to each element.
/// Return a new vector containing only the elements for which `f` returns `true`.
public macro fun filter<$T: drop>($v: vector<$T>, $f: |&$T| -> bool): vector<$T> {
    let v = $v;
    let mut r = vector[];
    v.do!(|e| if ($f(&e)) r.push_back(e));
    r
}

/// Split the vector `v` into two vectors by applying the function `f` to each element.
/// Return a tuple containing two vectors: the first containing the elements for which `f` returns `true`,
/// and the second containing the elements for which `f` returns `false`.
public macro fun partition<$T>($v: vector<$T>, $f: |&$T| -> bool): (vector<$T>, vector<$T>) {
    let v = $v;
    let mut r1 = vector[];
    let mut r2 = vector[];
    v.do!(|e| if ($f(&e)) r1.push_back(e) else r2.push_back(e));
    (r1, r2)
}

/// Finds the index of first element in the vector `v` that satisfies the predicate `f`.
/// Returns `some(index)` if such an element is found, otherwise `none()`.
public macro fun find_index<$T>($v: &vector<$T>, $f: |&$T| -> bool): Option<u64> {
    let v = $v;
    'find_index: {
        v.length().do!(|i| if ($f(&v[i])) return 'find_index option::some(i));
        option::none()
    }
}

/// Finds all indices of elements in the vector `v` that satisfy the predicate `f`.
/// Returns a vector of indices of all found elements.
public macro fun find_indices<$T>($v: &vector<$T>, $f: |&$T| -> bool): vector<u64> {
    let v = $v;
    let mut indices = vector[];
    v.length().do!(|i| if ($f(&v[i])) indices.push_back(i));
    indices
}

/// Count how many elements in the vector `v` satisfy the predicate `f`.
public macro fun count<$T>($v: &vector<$T>, $f: |&$T| -> bool): u64 {
    let v = $v;
    let mut count = 0;
    v.do_ref!(|e| if ($f(e)) count = count + 1);
    count
}

/// Reduce the vector `v` to a single value by applying the function `f` to each element.
/// Similar to `fold_left` in Rust and `reduce` in Python and JavaScript.
public macro fun fold<$T, $Acc>($v: vector<$T>, $init: $Acc, $f: |$Acc, $T| -> $Acc): $Acc {
    let v = $v;
    let mut acc = $init;
    v.do!(|e| acc = $f(acc, e));
    acc
}

/// Concatenate the vectors of `v` into a single vector, keeping the order of the elements.
public fun flatten<T>(v: vector<vector<T>>): vector<T> {
    let mut r = vector[];
    v.do!(|u| r.append(u));
    r
}

/// Whether any element in the vector `v` satisfies the predicate `f`.
/// If the vector is empty, returns `false`.
public macro fun any<$T>($v: &vector<$T>, $f: |&$T| -> bool): bool {
    let v = $v;
    'any: {
        v.do_ref!(|e| if ($f(e)) return 'any true);
        false
    }
}

/// Whether all elements in the vector `v` satisfy the predicate `f`.
/// If the vector is empty, returns `true`.
public macro fun all<$T>($v: &vector<$T>, $f: |&$T| -> bool): bool {
    let v = $v;
    'all: {
        v.do_ref!(|e| if (!$f(e)) return 'all false);
        true
    }
}

/// Destroys two vectors `v1` and `v2` by calling `f` to each pair of elements.
/// Aborts if the vectors are not of the same length.
/// The order of elements in the vectors is preserved.
public macro fun zip_do<$T1, $T2, $R: drop>(
    $v1: vector<$T1>,
    $v2: vector<$T2>,
    $f: |$T1, $T2| -> $R,
) {
    let v1 = $v1;
    let mut v2 = $v2;
    v2.reverse();
    let len = v1.length();
    assert!(len == v2.length());
    v1.do!(|el1| $f(el1, v2.pop_back()));
    v2.destroy_empty();
}

/// Destroys two vectors `v1` and `v2` by calling `f` to each pair of elements.
/// Aborts if the vectors are not of the same length.
/// Starts from the end of the vectors.
public macro fun zip_do_reverse<$T1, $T2, $R: drop>(
    $v1: vector<$T1>,
    $v2: vector<$T2>,
    $f: |$T1, $T2| -> $R,
) {
    let v1 = $v1;
    let mut v2 = $v2;
    let len = v1.length();
    assert!(len == v2.length());
    v1.destroy!(|el1| $f(el1, v2.pop_back()));
}

/// Iterate through `v1` and `v2` and apply the function `f` to references of each pair of
/// elements. The vectors are not modified.
/// Aborts if the vectors are not of the same length.
/// The order of elements in the vectors is preserved.
public macro fun zip_do_ref<$T1, $T2, $R: drop>(
    $v1: &vector<$T1>,
    $v2: &vector<$T2>,
    $f: |&$T1, &$T2| -> $R,
) {
    let v1 = $v1;
    let v2 = $v2;
    let len = v1.length();
    assert!(len == v2.length());
    len.do!(|i| $f(&v1[i], &v2[i]));
}

/// Iterate through `v1` and `v2` and apply the function `f` to mutable references of each pair
/// of elements. The vectors may be modified.
/// Aborts if the vectors are not of the same length.
/// The order of elements in the vectors is preserved.
public macro fun zip_do_mut<$T1, $T2, $R: drop>(
    $v1: &mut vector<$T1>,
    $v2: &mut vector<$T2>,
    $f: |&mut $T1, &mut $T2| -> $R,
) {
    let v1 = $v1;
    let v2 = $v2;
    let len = v1.length();
    assert!(len == v2.length());
    len.do!(|i| $f(&mut v1[i], &mut v2[i]));
}

/// Destroys two vectors `v1` and `v2` by applying the function `f` to each pair of elements.
/// The returned values are collected into a new vector.
/// Aborts if the vectors are not of the same length.
/// The order of elements in the vectors is preserved.
public macro fun zip_map<$T1, $T2, $U>(
    $v1: vector<$T1>,
    $v2: vector<$T2>,
    $f: |$T1, $T2| -> $U,
): vector<$U> {
    let mut r = vector[];
    zip_do!($v1, $v2, |el1, el2| r.push_back($f(el1, el2)));
    r
}

/// Iterate through `v1` and `v2` and apply the function `f` to references of each pair of
/// elements. The returned values are collected into a new vector.
/// Aborts if the vectors are not of the same length.
/// The order of elements in the vectors is preserved.
public macro fun zip_map_ref<$T1, $T2, $U>(
    $v1: &vector<$T1>,
    $v2: &vector<$T2>,
    $f: |&$T1, &$T2| -> $U,
): vector<$U> {
    let mut r = vector[];
    zip_do_ref!($v1, $v2, |el1, el2| r.push_back($f(el1, el2)));
    r
}

/// Performs an in-place insertion sort on the vector `v` using the comparison function `le`.
/// The sort is stable, meaning that equal elements will maintain their relative order.
///
/// Please, note that the comparison function `le` expects less or equal, not less.
///
/// Example:
/// ```
/// let mut v = vector[2, 1, 3];
/// v.insertion_sort_by(|a, b| a <= b);
/// assert!(v == vector[1, 2, 3]);
/// ```
///
/// Insertion sort is efficient for small vectors (~30 or less elements), and can
/// be faster than merge sort for almost sorted vectors (e.g. when the vector is
/// already sorted or nearly sorted).
public macro fun insertion_sort_by<$T>($v: &mut vector<$T>, $le: |&$T, &$T| -> bool) {
    let v = $v;
    let n = v.length();
    if (n < 2) return;
    // do insertion sort
    let mut i = 1;
    while (i < n) {
        let mut j = i;
        while (j > 0 && !$le(&v[j - 1], &v[j])) {
            v.swap(j, j - 1);
            j = j - 1;
        };
        i = i + 1;
    }
}

/// Performs an in-place merge sort on the vector `v` using the comparison function `le`.
/// Merge sort is efficient for large vectors, and is a stable sort.
///
/// Please, note that the comparison function `le` expects less or equal, not less.
///
/// Example:
/// ```
/// let mut v = vector[2, 1, 3];
/// v.merge_sort_by(|a, b| a <= b);
/// assert!(v == vector[1, 2, 3]);
/// ```
///
/// Merge sort performs better than insertion sort for large vectors (~30 elements or more).
public macro fun merge_sort_by<$T>($v: &mut vector<$T>, $le: |&$T, &$T| -> bool) {
    let v = $v;
    let n = v.length();
    if (n < 2) return;

    let mut flags = vector[false];
    let mut starts = vector[0];
    let mut ends = vector[n];
    while (!flags.is_empty()) {
        let (halves_sorted, start, end) = (flags.pop_back(), starts.pop_back(), ends.pop_back());
        let mid = (start + end) / 2;
        if (halves_sorted) {
            let mut mid = mid;
            let mut l = start;
            let mut r = mid;
            while (l < mid && r < end) {
                if ($le(&v[l], &v[r])) {
                    l = l + 1;
                } else {
                    let mut i = r;
                    while (i > l) {
                        v.swap(i, i - 1);
                        i = i - 1;
                    };

                    l = l + 1;
                    mid = mid + 1;
                    r = r + 1;
                }
            }
        } else {
            // set up the "merge"
            flags.push_back(true);
            starts.push_back(start);
            ends.push_back(end);
            // set up the recursive calls
            // v[start..mid]
            if (mid - start > 1) {
                flags.push_back(false);
                starts.push_back(start);
                ends.push_back(mid);
            };
            // v[mid..end]
            if (end - mid > 1) {
                flags.push_back(false);
                starts.push_back(mid);
                ends.push_back(end);
            }
        }
    }
}

/// Check if the vector `v` is sorted in non-decreasing order according to the comparison
/// function `le` (les). Returns `true` if the vector is sorted, `false` otherwise.
public macro fun is_sorted_by<$T>($v: &vector<$T>, $le: |&$T, &$T| -> bool): bool {
    let v = $v;
    let n_minus_1 = v.length().max(1) - 1;
    'is_sorted_by: {
        n_minus_1.do!(|i| if (!$le(&v[i], &v[i + 1])) return 'is_sorted_by false);
        true
    }
}
