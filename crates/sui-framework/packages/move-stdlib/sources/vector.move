// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[defines_primitive(vector)]
/// A variable-sized container that can hold any type. Indexing is 0-based, and
/// vectors are growable. This module has many native functions.
module std::vector {

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
    native public fun empty<Element>(): vector<Element>;

    #[bytecode_instruction]
    /// Return the length of the vector.
    native public fun length<Element>(v: &vector<Element>): u64;

    #[syntax(index)]
    #[bytecode_instruction]
    /// Acquire an immutable reference to the `i`th element of the vector `v`.
    /// Aborts if `i` is out of bounds.
    native public fun borrow<Element>(v: &vector<Element>, i: u64): &Element;

    #[bytecode_instruction]
    /// Add element `e` to the end of the vector `v`.
    native public fun push_back<Element>(v: &mut vector<Element>, e: Element);

    #[syntax(index)]
    #[bytecode_instruction]
    /// Return a mutable reference to the `i`th element in the vector `v`.
    /// Aborts if `i` is out of bounds.
    native public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;

    #[bytecode_instruction]
    /// Pop an element from the end of vector `v`.
    /// Aborts if `v` is empty.
    native public fun pop_back<Element>(v: &mut vector<Element>): Element;

    #[bytecode_instruction]
    /// Destroy the vector `v`.
    /// Aborts if `v` is not empty.
    native public fun destroy_empty<Element>(v: vector<Element>);

    #[bytecode_instruction]
    /// Swaps the elements at the `i`th and `j`th indices in the vector `v`.
    /// Aborts if `i` or `j` is out of bounds.
    native public fun swap<Element>(v: &mut vector<Element>, i: u64, j: u64);

    /// Return an vector of size one containing element `e`.
    public fun singleton<Element>(e: Element): vector<Element> {
        let mut v = empty();
        v.push_back(e);
        v
    }

    /// Reverses the order of the elements in the vector `v` in place.
    public fun reverse<Element>(v: &mut vector<Element>) {
        let len = v.length();
        if (len == 0) return ();

        let mut front_index = 0;
        let mut back_index = len -1;
        while (front_index < back_index) {
            v.swap(front_index, back_index);
            front_index = front_index + 1;
            back_index = back_index - 1;
        }
    }

    /// Pushes all of the elements of the `other` vector into the `lhs` vector.
    public fun append<Element>(lhs: &mut vector<Element>, mut other: vector<Element>) {
        other.reverse();
        while (!other.is_empty()) lhs.push_back(other.pop_back());
        other.destroy_empty();
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
        while (i < len) v.swap(i, { i = i + 1; i });
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
        assert!(!v.is_empty(), EINDEX_OUT_OF_BOUNDS);
        let last_idx = v.length() - 1;
        v.swap(i, last_idx);
        v.pop_back()
    }

    // === Macros ===

    /// Destroy the vector `v` by calling `f` on each element and then destroying the vector.
    /// Does not preserve the order of elements in the vector (starts from the end of the vector).
    public macro fun destroy<$T>($v: vector<$T>, $f: |$T|) {
        let mut v = $v;
        while (!v.is_empty()) $f(v.pop_back());
        v.destroy_empty();
    }

    /// Destroy the vector `v` by calling `f` on each element and then destroying the vector.
    /// Preserves the order of elements in the vector.
    public macro fun do<$T>($v: vector<$T>, $f: |$T|) {
        let mut v = $v;
        v.reverse();
        while (!v.is_empty()) $f(v.pop_back());
        v.destroy_empty();
    }

    /// Perform an action `f` on each element of the vector `v`. The vector is not modified.
    public macro fun do_ref<$T>($v: &vector<$T>, $f: |&$T|) {
        let v = $v;
        v.length().do!(|i| $f(&v[i]))
    }

    /// Perform an action `f` on each element of the vector `v`.
    /// The function `f` takes a mutable reference to the element.
    public macro fun do_mut<$T>($v: &mut vector<$T>, $f: |&mut $T|) {
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

    /// Reduce the vector `v` to a single value by applying the function `f` to each element.
    /// Similar to `fold_left` in Rust and `reduce` in Python and JavaScript.
    public macro fun fold<$T, $Acc>($v: vector<$T>, $init: $Acc, $f: |$Acc, $T| -> $Acc): $Acc {
        let v = $v;
        let mut acc = $init;
        v.do!(|e| acc = $f(acc, e));
        acc
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
}
