// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Priority queue implemented using a max heap.
module sui::priority_queue {
    use std::vector;

    /// For when heap is empty and there's no data to pop.
    const EPopFromEmptyHeap: u64 = 0;

    /// For when heap is empty and there's no data to peek.
    const EPeekFromEmptyHeap: u64 = 1;

    const U64_MAX: u64 = 18446744073709551615;

    struct PriorityQueue<T> has store {
        entries: vector<Entry<T>>,
        ascending: bool,
    }

    struct Entry<T> has store {
        priority: u64,
        value: T,
    }

    /// Create a new priority queue from the entries in ascending/descending order.
    public fun new<T>(entries: vector<Entry<T>>, ascending: bool) : PriorityQueue<T> {
        let len = vector::length(&entries);

        // If ascending, flip the priorities
        // This means ascending will be slighly costlier though
        if (ascending) {
            let i = 0;
            while (i < len) {
                let elem = vector::borrow_mut(&mut entries, i);
                elem.priority = U64_MAX - elem.priority;
                i = i + 1;
            }
        };
        let i = len / 2;
        while (i > 0) {
            i = i - 1;
            max_heapify_recursive(&mut entries, len, i);
        };
        PriorityQueue { entries, ascending }
    }

    /// Get the length of the priority queue
    public fun length<T>(pq: &PriorityQueue<T>) : u64 {
        vector::length(&pq.entries)
    }

    /// Check if the priority queue is empty
    public fun empty<T>(pq: &PriorityQueue<T>) : bool {
        vector::length(&pq.entries) == 0
    }

    /// Drains the pq and inner vec and returns the items
    public fun drain<T>(pq: PriorityQueue<T>) : vector<T> {
        let len = vector::length(&pq.entries);
        let i = 0;
        let res = vector::empty();
        while (i < len) {
            let Entry { priority: _, value } = vector::pop_back(&mut pq.entries);
            vector::push_back(&mut res, value);
            i = i + 1;
        };
        let PriorityQueue {entries, ascending: _} = pq;
        vector::destroy_empty(entries);
        res
    }

    /// Peek the entry with the highest (or lowest if ascending) priority value.
    public fun peek<T>(pq: &mut PriorityQueue<T>) : (u64, &T) {
        assert!(!empty(pq), EPeekFromEmptyHeap);

        let Entry { priority, value } = vector::borrow(&pq.entries, 0);
        let priority = *priority;
        if (pq.ascending) {
            priority = U64_MAX - priority;
        };
        (priority, value)
    }

    /// Pop the entry with the highest (or lowest if ascending) priority value.
    public fun pop<T>(pq: &mut PriorityQueue<T>) : (u64, T) {
        let len = vector::length(&pq.entries);
        assert!(len > 0, EPopFromEmptyHeap);
        let Entry { priority, value } = vector::remove(&mut pq.entries, 0);
        max_heapify_recursive(&mut pq.entries, len - 1, 0);
        if (pq.ascending) {
            priority = U64_MAX - priority;
        };
        (priority, value)
    }

    /// Insert a new entry into the queue.
    public fun insert<T>(pq: &mut PriorityQueue<T>, priority: u64, value: T) {
        if (pq.ascending) {
            priority = U64_MAX - priority;
        };
        vector::push_back(&mut pq.entries, Entry { priority, value});
        let index = vector::length(&pq.entries) - 1;
        restore_heap_recursive(&mut pq.entries, index);
    }

    public fun new_entry<T>(priority: u64, value: T): Entry<T> {
        Entry { priority, value }
    }

    public fun create_entries<T: drop>(p: vector<u64>, v: vector<T>): vector<Entry<T>> {
        let len = vector::length(&p);
        assert!(vector::length(&v) == len, 0);
        let res = vector::empty();
        let i = 0;
        while (i < len) {
            let priority = vector::remove(&mut p, 0);
            let value = vector::remove(&mut v, 0);
            vector::push_back(&mut res, Entry { priority, value });
            i = i + 1;
        };
        res
    }

    // TODO: implement iterative version too and see performance difference.
    fun restore_heap_recursive<T>(v: &mut vector<Entry<T>>, i: u64) {
        if (i == 0) {
            return
        };
        let parent = (i - 1) / 2;

        // If new elem is greater than its parent, swap them and recursively
        // do the restoration upwards.
        if (vector::borrow(v, i).priority > vector::borrow(v, parent).priority) {
            vector::swap(v, i, parent);
            restore_heap_recursive(v, parent);
        }
    }

    // TODO: improve with iterative version
    fun max_heapify_recursive<T>(v: &mut vector<Entry<T>>, len: u64, i: u64) {
        if (len == 0) {
            return
        };
        assert!(i < len, 1);
        let left = i * 2 + 1;
        let right = left + 1;
        let max = i;
        if (left < len && vector::borrow(v, left).priority> vector::borrow(v, max).priority) {
            max = left;
        };
        if (right < len && vector::borrow(v, right).priority > vector::borrow(v, max).priority) {
            max = right;
        };
        if (max != i) {
            vector::swap(v, max, i);
            max_heapify_recursive(v, len, max);
        }
    }

    #[test]
    fun test_pq() {
        // Descending
        let h = new(create_entries(vector[3,1,4,2,5,2], vector[10, 20, 30, 40, 50, 60]), false);
        assert!(length(&h) == 6, 0);
        assert!(!empty(&h), 0);
        check_peek_pop(&mut h, 5, 50);
        check_peek_pop(&mut h, 4, 30);
        check_peek_pop(&mut h, 3, 10);
        insert(&mut h, 7, 70);
        check_peek_pop(&mut h, 7, 70);
        check_peek_pop(&mut h, 2, 40);
        insert(&mut h, 0, 80);
        check_peek_pop(&mut h, 2, 60);
        check_peek_pop(&mut h, 1, 20);
        check_peek_pop(&mut h, 0, 80);
        let _ = drain(h);


        let h = new(create_entries(vector[5,3,1,2,4], vector[10, 20, 30, 40, 50]), false);
        assert!(length(&h) == 5, 0);
        assert!(!empty(&h), 0);    
        check_peek_pop(&mut h, 5, 10);
        check_peek_pop(&mut h, 4, 50);
        check_peek_pop(&mut h, 3, 20);
        check_peek_pop(&mut h, 2, 40);
        check_peek_pop(&mut h, 1, 30);
        assert!(length(&h) == 0, 0);
        assert!(empty(&h), 0);
        let _ = drain(h);

        // Ascending
        let h = new(create_entries(vector[1,2,3,4,5,5], vector[60, 50, 40, 30, 20, 10]), true);
        assert!(length(&h) == 6, 0);
        assert!(!empty(&h), 0);  
        check_peek_pop(&mut h, 1, 60);
        check_peek_pop(&mut h, 2, 50);
        check_peek_pop(&mut h, 3, 40);
        insert(&mut h, 7, 70);
        check_peek_pop(&mut h, 4, 30);
        check_peek_pop(&mut h, 5, 20);
        insert(&mut h, 0, 80);
        check_peek_pop(&mut h, 0, 80);
        check_peek_pop(&mut h, 5, 10);
        check_peek_pop(&mut h, 7, 70);
        assert!(length(&h) == 0, 0);
        assert!(empty(&h), 0);
        let _ = drain(h);

        let h = new(create_entries(vector[5,3,1,2,4], vector[10, 20, 30, 40, 50]), true);
        assert!(length(&h) == 5, 0);
        assert!(!empty(&h), 0);  
        check_peek_pop(&mut h, 1, 30);
        check_peek_pop(&mut h, 2, 40);
        check_peek_pop(&mut h, 3, 20);
        check_peek_pop(&mut h, 4, 50);
        check_peek_pop(&mut h, 5, 10);
        assert!(length(&h) == 0, 0);
        assert!(empty(&h), 0);
        let _ = drain(h);

        // Drain a heap
        let h = new(create_entries(vector[1,2,3,4,5,5], vector[60, 50, 40, 30, 20, 10]), true);
        assert!(length(&h) == 6, 0);
        assert!(!empty(&h), 0);  
        check_peek_pop(&mut h, 1, 60);
        check_peek_pop(&mut h, 2, 50);
        check_peek_pop(&mut h, 3, 40);
        insert(&mut h, 0, 80);
        check_peek_pop(&mut h, 0, 80);
        check_peek_pop(&mut h, 4, 30);

        let drained = drain(h);
        assert!(vector::length(&drained) == 2, 0);
        let a0 = vector::pop_back(&mut drained);
        let a1 = vector::pop_back(&mut drained);

        assert!(((a0 == 20) && (a1 == 10)) || ((a0 == 10) && (a1 == 20)), 0);
    }


    #[test_only]
    fun check_peek_pop(h: &mut PriorityQueue<u64>, expected_priority: u64, expected_value: u64) {
        let ln = length(h);
        let (priority, value) = peek(h);
        assert!(priority == expected_priority, 0);
        assert!(*value == expected_value, 0);
        // Ensure peek does not alter length
        assert!(length(h) == ln, 0);

        let (priority, value) = pop(h);
        assert!(priority == expected_priority, 0);
        assert!(value == expected_value, 0);
    }
}
