// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::sized_vector {
    use std::option;
    use std::option::Option;
    use std::vector;
    #[test_only]
    use sui::test_utils::assert_eq;

    struct SizedVector<T> has store, drop {
        data: vector<Option<T>>,
        head: u64,
        size: u64,
    }

    public fun new<T>(capacity: u64): SizedVector<T> {
        let (i, data) = (0, vector[]);
        while (i < capacity) {
            vector::push_back(&mut data, option::none<T>());
            i = i + 1
        };
        SizedVector<T> {
            data,
            head: 0,
            size: 0
        }
    }

    /// Pop an element from the head of vector `v`.
    /// This is O(1) and preserves ordering of elements in the vector.
    public fun pop_first<T>(self: &mut SizedVector<T>): T {
        let capacity = vector::length(&self.data);
        let option = vector::borrow_mut(&mut self.data, self.head);
        self.head = if (self.head == capacity - 1) {
            0
        }else {
            self.head + 1
        };
        self.size = self.size - 1;
        option::extract(option)
    }

    /// Add element `e` to the end of the vector `v`.
    /// Head will be poped if the vector exceed capacity
    public fun push_back<T>(self: &mut SizedVector<T>, element: T): Option<T> {
        let poped = if (self.size == vector::length(&self.data)) {
            option::some(pop_first(self))
        }else {
            option::none()
        };
        let size = self.size;
        let index = map_index(self, size);
        let option = vector::borrow_mut(&mut self.data, index);
        option::fill(option, element);
        self.size = size + 1;
        poped
    }

    public fun pop_back<T>(self: &mut SizedVector<T>): T {
        let tail = tail_index(self);
        let option = vector::borrow_mut(&mut self.data, tail);
        self.size = self.size - 1;
        option::extract(option)
    }

    public fun borrow<T>(self: &SizedVector<T>, index: u64): &Option<T> {
        vector::borrow(&self.data, map_index(self, index))
    }

    public fun borrow_mut<T>(self: &mut SizedVector<T>, index: u64): &mut Option<T> {
        let index = map_index(self, index);
        vector::borrow_mut(&mut self.data, index)
    }

    public fun length<T>(self: &SizedVector<T>): u64 {
        self.size
    }

    fun map_index<T>(self: &SizedVector<T>, index: u64): u64 {
        let capacity = vector::length(&self.data);
        if (index + self.head >= capacity) {
            index + self.head - capacity
        } else {
            index + self.head
        }
    }

    fun tail_index<T>(self: &SizedVector<T>): u64 {
        if (self.head == 0) {
            if (self.size == 0) {
                0
            }else {
                self.size - 1
            }
        }else {
            map_index(self, self.size - 1)
        }
    }

    #[test]
    fun test_push_pop() {
        let cv = new<u64>(10);
        let i = 0;

        // Populate vector
        while (i < 10) {
            push_back(&mut cv, i);
            i = i + 1;
        };

        // Pop back
        while (i > 0) {
            i = i - 1;
            assert_eq(pop_back(&mut cv), i);
        };
    }

    #[test]
    fun test_push_back() {
        let cv = new<u64>(10);
        let i = 0;

        // Populate vector
        while (i < 10) {
            push_back(&mut cv, i);
            i = i + 1;
        };

        // push back more, first 5 elements will be poped due to capacity
        i = 0;
        while (i < 5) {
            let poped = push_back(&mut cv, i + 10);
            assert_eq(poped, option::some(i));
            i = i + 1;
        };

        // Check the rest of the elements
        i = 5;
        while (i < 15) {
            let poped = pop_first(&mut cv);
            assert_eq(poped, i);
            i = i + 1;
        };
    }

    #[test]
    fun test_push_pop_first() {
        let cv = new<u64>(10);
        let i = 0;

        // Populate vector
        while (i < 10) {
            push_back(&mut cv, i);
            i = i + 1;
        };

        // Pop first
        i = 0;
        while (i < 2) {
            let p = pop_first(&mut cv);
            assert_eq(p, i);
            i = i + 1;
        };
    }

    #[test]
    fun test_push_pop_mix() {
        let cv = new<u64>(10);
        let i = 0;

        // Populate vector
        while (i < 10) {
            push_back(&mut cv, i);
            i = i + 1;
        };

        // Pop first 5 elements, data should be [5,6,7,8,9]
        i = 0;
        while (i < 5) {
            let p = pop_first(&mut cv);
            assert_eq(p, i);
            i = i + 1;
        };

        // push 5 new elements, data should be [5,6,7,8,9,10,11,12,13,14]
        i = 10;
        while (i < 15) {
            push_back(&mut cv, i);
            i = i + 1;
        };

        // pop back and check all elements
        i = 14;
        while (i > 4) {
            let p = pop_back(&mut cv);
            assert_eq(p, i);
            i = i - 1;
        };

        assert_eq(0, length(&cv))
    }

    #[test]
    fun test_push_full_vector() {
        let cv = new<u64>(10);
        let i = 0;
        // Populate vector
        while (i < 10) {
            push_back(&mut cv, i);
            i = i + 1;
        };
        assert_eq(10, length(&cv));
        let poped = push_back(&mut cv, i);
        assert!(option::is_some(&poped), 0);
        assert_eq(10, length(&cv));
    }
}


