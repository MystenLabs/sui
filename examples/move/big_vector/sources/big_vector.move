// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// BigVector is an arbitrary sized vector-like data structure,
/// implemented using an on-chain B+ Tree to support almost constant
/// time (log base max_fan_out) random access, insertion and removal.
///
/// Iteration is supported by exposing access to leaf nodes (slices).
/// Finding the initial slice can be done in almost constant time, and
/// subsequently finding the previous or next slice can also be done
/// in constant time.
///
/// Nodes in the B+ Tree are stored as individual dynamic fields
/// hanging off the `BigVector`.
///
/// Note: The index type is `u128`, but the length is stored as `u64`
/// because the expectation is that indices are sparsely distributed.
module big_vector::big_vector {
    use sui::dynamic_field as df;

    use fun sui::object::new as TxContext.new;

    use fun big_vector::utils::pop_until as vector.pop_until;
    use fun big_vector::utils::pop_n as vector.pop_n;

    public struct BigVector<phantom E: store> has key, store {
        id: UID,

        /// How deep the tree structure is.
        depth: u8,

        /// Total number of elements that this vector contains, not
        /// including gaps in the vector.
        length: u64,

        /// Max size of leaf nodes (counted in number of elements, `E`).
        max_slice_size: u64,

        /// Max size of interior nodes (counted in number of children).
        max_fan_out: u64,

        /// ID of the tree's root structure. Value of `NO_SLICE` means
        /// there's no root.
        root_id: u64,

        /// The last node ID that was allocated.
        last_id: u64,
    }

    /// A node in the B+ tree.
    ///
    /// If representing a leaf node, there are as many keys as values
    /// (such that `keys[i]` is the key corresponding to `vals[i]`).
    ///
    /// A `Slice<u64>` can also represent an interior node, in which
    /// case `vals` contain the IDs of its children and `keys`
    /// represent the partitions between children. There will be one
    /// fewer key than value in this configuration.
    public struct Slice<E: store> has store, drop {
        /// Previous node in the intrusive doubly-linked list data
        /// structure.
        prev: u64,

        /// Next node in the intrusive doubly-linked list data
        /// structure.
        next: u64,

        keys: vector<u128>,
        vals: vector<E>,
    }

    /// Wrapper type around indices for slices. The internal index is
    /// the ID of the dynamic field containing the slice.
    public struct SliceRef has copy, drop, store { ix: u64 }

    // === Error Codes ===

    /// Max Slice Size provided is too small.
    const ESliceTooSmall: u64 = 0;

    /// Max Slice Size provided is too big.
    const ESliceTooBig: u64 = 1;

    /// Max Fan-out provided is too small.
    const EFanOutTooSmall: u64 = 2;

    /// Max Fan-out provided is too big.
    const EFanOutTooBig: u64 = 3;

    /// `BigVector` is not empty.
    const ENotEmpty: u64 = 4;

    /// Key not found in `BigVector`.
    const ENotFound: u64 = 5;

    /// Key already exists in `BigVector`.
    const EExists: u64 = 6;

    /// Found a node in an unexpected state during removal (namely, we
    /// tried to remove from a node's child and found that it had
    /// become empty, which should not be possible).
    const EBadRemove: u64 = 7;

    /// Found a pair of nodes that are expected to be adjacent but
    /// whose linked list pointers don't match up.
    const ENotAdjacent: u64 = 8;

    /// Tried to redistribute between two nodes, but the operation
    /// would have had no effect.
    const EBadRedistribution: u64 = 9;

    // === Constants ===

    /// Sentinel representing the absence of a slice.
    const NO_SLICE: u64 = 0;

    /// We will accommodate at least this much fan out before
    /// splitting interior nodes, so that after the split, we don't
    /// get an interior node that contains only one child.
    const MIN_FAN_OUT: u64 = 4;

    /// Internal nodes of `BigVector` can't have more children than
    /// this, to avoid hitting object size limits.
    const MAX_FAN_OUT: u64 = 4096;

    /// Leaf nodes of `BigVector` can't be bigger than this, to avoid
    /// hitting object size limits.
    const MAX_SLICE_SIZE: u64 = 256 * 1024;

    // === Removal fix-up strategies ===

    /// 0b000: No fix-up.
    const RM_FIX_NOTHING: u8 = 0;

    /// 0b001: Node is completely empty (applies only to root).
    const RM_FIX_EMPTY: u8 = 1;

    /// 0b010: Stole a key from the left neighbour, additional value
    /// is the new pivot after the steal.
    const RM_FIX_STEAL_L: u8 = 2;

    /// 0b011: Stole a key from the right neighbour, additional value
    /// is the new pivot after the steal.
    const RM_FIX_STEAL_R: u8 = 3;

    /// 0b100: Merged with the left neighbour.
    const RM_FIX_MERGE_L: u8 = 4;

    /// 0b101: Merged with the right neighbour.
    const RM_FIX_MERGE_R: u8 = 5;

    // === Constructors ===

    /// Construct a new, empty `BigVector`. `max_slice_size` contains
    /// the maximum size of its leaf nodes, and `max_fan_out` contains
    /// the maximum fan-out of its interior nodes.
    public fun empty<E: store>(
        max_slice_size: u64,
        max_fan_out: u64,
        ctx: &mut TxContext,
    ): BigVector<E> {
        assert!(0 < max_slice_size, ESliceTooSmall);
        assert!(max_slice_size <= MAX_SLICE_SIZE, ESliceTooBig);
        assert!(MIN_FAN_OUT <= max_fan_out, EFanOutTooSmall);
        assert!(max_fan_out <= MAX_FAN_OUT, EFanOutTooBig);

        BigVector {
            id: ctx.new(),

            depth: 0,
            length: 0,

            max_slice_size,
            max_fan_out,

            root_id: NO_SLICE,
            last_id: NO_SLICE,
        }
    }

    /// Destroy `self` as long as it is empty, even if its elements
    /// are not droppable. Fails if `self` is not empty.
    public fun destroy_empty<E: store>(self: BigVector<E>) {
        let BigVector {
            id,

            depth: _,
            length,
            max_slice_size: _,
            max_fan_out: _,

            root_id: _,
            last_id: _,
        } = self;

        assert!(length == 0, ENotEmpty);
        id.delete();
    }

    /// Destroy `self`, even if it contains elements, as long as they
    /// are droppable.
    public fun drop<E: store + drop>(self: BigVector<E>) {
        let BigVector {
            mut id,

            depth,
            length: _,
            max_slice_size: _,
            max_fan_out: _,

            root_id,
            last_id: _,
        } = self;

        drop_slice<E>(&mut id, depth, root_id);
        id.delete();
    }

    // === BigVector Accessors ===

    /// Whether `self` contains no elements or not.
    public fun is_empty<E: store>(self: &BigVector<E>): bool {
        self.length == 0
    }

    /// The number of elements contained in `self`.
    public fun length<E: store>(self: &BigVector<E>): u64 {
        self.length
    }

    /// The number of nodes between the root and the leaves in `self`.
    /// This is within a constant factor of log base `max_fan_out` of
    /// the length.
    public fun depth<E: store>(self: &BigVector<E>): u8 {
        self.depth
    }

    /// Check for the presence of `key` in `self`.
    public fun contains<E: store>(self: &BigVector<E>, key: u128): bool {
        let (ref, off) = self.slice_following(key);

        if (ref.is_null()) {
            return false
        };

        let slice = self.borrow_slice(ref);
        off < slice.length() && slice.key(off) == key
    }

    #[syntax(index)]
    /// Access the element at index `ix` in `self`.
    public fun borrow<E: store>(self: &BigVector<E>, ix: u128): &E {
        let (ref, offset) = self.slice_around(ix);
        let slice = self.borrow_slice(ref);
        &slice[offset]
    }

    #[syntax(index)]
    /// Access the element at index `ix` in `self`, mutably.
    public fun borrow_mut<E: store>(self: &mut BigVector<E>, ix: u128): &mut E {
        let (ref, offset) = self.slice_around(ix);
        let slice = self.borrow_slice_mut(ref);
        &mut slice[offset]
    }

    // === BigVector Mutators ===

    /// Add `val` to `self` at index `key`. Aborts if `key` is already
    /// present in `self`.
    public fun insert<E: store>(self: &mut BigVector<E>, key: u128, val: E) {
        self.length = self.length + 1;

        if (self.root_id == NO_SLICE) {
            self.root_id = self.alloc(singleton(key, val));
            return
        };

        let (root_id, depth) = (self.root_id, self.depth);
        let (key, other) = self.slice_insert(root_id, depth, key, val);

        if (other != NO_SLICE) {
            self.root_id = self.alloc(branch(key, root_id, other));
            self.depth = self.depth + 1;
        }
    }

    /// Adds key value pairs from `keys` and `vals` to `self`.
    /// Requires that `keys` and `vals` have the same length, and that
    /// `keys` is in sorted order.
    ///
    /// Aborts if any of the keys are already present in `self`, or
    /// the requirements on `keys` and `vals` are not met.
    public fun insert_batch<E: store>(
        _self: &mut BigVector<E>,
        _keys: vector<u128>,
        _vals: vector<E>,
    ) {
        abort 0
    }

    /// Remove the element with key `key` from `self`, returning its
    /// value. Aborts if `key` is not found.
    public fun remove<E: store>(self: &mut BigVector<E>, key: u128): E {
        self.length = self.length - 1;

        if (self.root_id == NO_SLICE) {
            abort ENotFound
        };

        let (root_id, depth) = (self.root_id, self.depth);
        let (val, rm_fix, _) = self.slice_remove(
            NO_SLICE,
            0u128,
            root_id,
            0u128,
            NO_SLICE,
            depth,
            key,
        );

        if (rm_fix == RM_FIX_EMPTY) {
            if (self.depth == 0) {
                let Slice<E> {
                    prev: _,
                    next: _,
                    keys: _,
                    vals,
                } = df::remove(&mut self.id, root_id);

                // SAFETY: The slice is guaranteed to be empty because
                // it is a leaf and we received the RM_FIX_EMPTY
                // fix-up.
                vals.destroy_empty();

                self.root_id = NO_SLICE;
            } else {
                let mut root: Slice<u64> = df::remove(&mut self.id, root_id);
                self.root_id = root.vals.pop_back();
                self.depth = self.depth - 1;
            }
        };

        val
    }

    /// Remove the elements between `lo` (inclusive) and `hi`
    /// (exclusive) from `self`.
    public fun remove_range<E: store + drop>(
        _self: &mut BigVector<E>,
        _lo: u128,
        _hi: u128,
    ) {
        abort 0
    }

    /// Remove elements from `self` at the indices in `keys`,
    /// returning the associated values.
    ///
    /// Aborts if any of the keys are not found.
    public fun remove_batch<E: store>(
        _self: &mut BigVector<E>,
        _keys: vector<u128>,
    ): vector<E> {
        abort 0
    }

    // === SliceRef ===

    /// Find the slice that contains the key-value pair for `key`,
    /// assuming it exists in the data structure. Returns the
    /// reference to the slice and the local offset within the slice
    /// if it exists, aborts with `ENotFound` otherwise.
    public fun slice_around<E: store>(
        self: &BigVector<E>,
        key: u128,
    ): (SliceRef, u64) {
        if (self.root_id == NO_SLICE) {
            abort ENotFound
        };

        let (ix, leaf, off) = self.find_leaf(key);

        if (off >= leaf.keys.length()) {
            abort ENotFound
        } else if (key != leaf.keys[off]) {
            abort ENotFound
        };

        (SliceRef { ix }, off)
    }

    /// Find the slice that contains the key-value pair corresponding
    /// to the next key in `self` at or after `key`. Returns the
    /// reference to the slice and the local offset within the slice
    /// if it exists, or (NO_SLICE, 0), if there is no matching
    /// key-value pair.
    public fun slice_following<E: store>(
        self: &BigVector<E>,
        key: u128,
    ): (SliceRef, u64) {
        if (self.root_id == NO_SLICE) {
            return (SliceRef { ix: NO_SLICE }, 0)
        };

        let (ix, leaf, off) = self.find_leaf(key);
        if (off >= leaf.keys.length()) {
            (leaf.next(), 0)
        } else {
            (SliceRef { ix }, off)
        }
    }

    /// Borrow a slice from this vector.
    public fun borrow_slice<E: store>(
        self: &BigVector<E>,
        ref: SliceRef,
    ): &Slice<E> {
        df::borrow(&self.id, ref.ix)
    }

    /// Borrow a slice from this vector, mutably.
    public fun borrow_slice_mut<E: store>(
        self: &mut BigVector<E>,
        ref: SliceRef,
    ): &mut Slice<E> {
        df::borrow_mut(&mut self.id, ref.ix)
    }

    // === Receiver function aliases ===

    public use fun slice_is_null as SliceRef.is_null;
    public use fun slice_is_leaf as Slice.is_leaf;
    public use fun slice_next as Slice.next;
    public use fun slice_prev as Slice.prev;
    public use fun slice_length as Slice.length;
    public use fun slice_key as Slice.key;
    public use fun slice_borrow as Slice.borrow;
    public use fun slice_borrow_mut as Slice.borrow_mut;
    public use fun slice_bisect_left as Slice.bisect_left;
    public use fun slice_bisect_right as Slice.bisect_right;

    // === Slice Accessors ===


    /// Returns whether the SliceRef points to an actual slice, or the
    /// `NO_SLICE` sentinel. It is an error to attempt to borrow a
    /// slice from a `BigVector` if it doesn't exist.
    public fun slice_is_null(self: &SliceRef): bool {
        self.ix == NO_SLICE
    }

    /// Returns whether the slice is a leaf node or not. Leaf nodes
    /// have as many keys as values.
    public fun slice_is_leaf<E: store>(self: &Slice<E>): bool {
        self.vals.length() == self.keys.length()
    }

    /// Reference to the next (neighbouring) slice to this one.
    public fun slice_next<E: store>(self: &Slice<E>): SliceRef {
        SliceRef { ix: self.next }
    }

    /// Reference to the previous (neighbouring) slice to this one.
    public fun slice_prev<E: store>(self: &Slice<E>): SliceRef {
        SliceRef { ix: self.prev }
    }

    /// Number of children (values) in this slice.
    public fun slice_length<E: store>(self: &Slice<E>): u64 {
        self.vals.length()
    }

    /// Access a key from this slice, referenced by its offset, local
    /// to the slice.
    public fun slice_key<E: store>(self: &Slice<E>, ix: u64): u128 {
        self.keys[ix]
    }

    #[syntax(index)]
    /// Access a value from this slice, referenced by its offset,
    /// local to the slice.
    public fun slice_borrow<E: store>(self: &Slice<E>, ix: u64): &E {
        &self.vals[ix]
    }

    #[syntax(index)]
    /// Access a value from this slice, mutably, referenced by its
    /// offset, local to the slice.
    public fun slice_borrow_mut<E: store>(
        self: &mut Slice<E>,
        ix: u64,
    ): &mut E {
        &mut self.vals[ix]
    }

    // === Private Helpers ===

    /// Store `slice` as a dynamic field on `self`, and use its
    /// dynamic field ID to connect it into the doubly linked list
    /// structure at its level. Returns the ID of the slice to be used
    /// in a `SliceRef`.
    fun alloc<E: store, F: store>(
        self: &mut BigVector<E>,
        slice: Slice<F>,
    ): u64 {
        let prev = slice.prev;
        let next = slice.next;

        self.last_id = self.last_id + 1;
        df::add(&mut self.id, self.last_id, slice);
        let curr = self.last_id;

        if (prev != NO_SLICE) {
            let prev: &mut Slice<F> = df::borrow_mut(&mut self.id, prev);
            prev.next = curr;
        };

        if (next != NO_SLICE) {
            let next: &mut Slice<F> = df::borrow_mut(&mut self.id, next);
            next.prev = curr;
        };

        curr
    }

    /// Create a slice representing a leaf node containing a single
    /// key-value pair.
    fun singleton<E: store>(key: u128, val: E): Slice<E> {
        Slice {
            prev: NO_SLICE,
            next: NO_SLICE,
            keys: vector[key],
            vals: vector[val],
        }
    }

    /// Create a slice representing an interior node containing a
    /// single branch.
    fun branch(key: u128, left: u64, right: u64): Slice<u64> {
        Slice {
            prev: NO_SLICE,
            next: NO_SLICE,
            keys: vector[key],
            vals: vector[left, right],
        }
    }

    /// Recursively `drop` the nodes under the node at id `node`.
    /// Assumes that node has depth `depth` and is owned by `id`.
    fun drop_slice<E: store + drop>(id: &mut UID, depth: u8, slice: u64) {
        if (slice == NO_SLICE) {
            return
        } else if (depth == 0) {
            let _: Slice<E> = df::remove(id, slice);
        } else {
            let mut slice: Slice<u64> = df::remove(id, slice);
            while (!slice.vals.is_empty()) {
                drop_slice<E>(id, depth - 1, slice.vals.pop_back());
            }
        }
    }

    /// Find the leaf slice that would contain `key` if it existed in
    /// `self`. Returns the slice ref for the leaf, a reference to the
    /// leaf, and the offset in the leaf of the key (if the key were
    /// to exist in `self` it would appear here).
    ///
    /// Assumes `self` is non-empty.
    fun find_leaf<E: store>(
        self: &BigVector<E>,
        key: u128,
    ): (u64, &Slice<E>, u64) {
        let (mut slice_id, mut depth) = (self.root_id, self.depth);

        while (depth > 0) {
            let node: &Slice<u64> = df::borrow(&self.id, slice_id);
            let off = node.bisect_right(key);
            slice_id = node.vals[off];
            depth = depth - 1;
        };

        let leaf: &Slice<E> = df::borrow(&self.id, slice_id);
        let off = leaf.bisect_left(key);

        (slice_id, leaf, off)
    }

    /// Find the position in `slice.keys` of `key` if it exists, or
    /// the minimal position it should be inserted in to maintain
    /// sorted order.
    fun slice_bisect_left<E: store>(self: &Slice<E>, key: u128): u64 {
        let (mut lo, mut hi) = (0, self.keys.length());

        // Invariant: keys[0, lo) < key <= keys[hi, ..)
        while (lo < hi) {
            let mid = (hi - lo) / 2 + lo;
            if (key <= self.keys[mid]) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        };

        lo
    }

    /// Find the largest index in `slice.keys` to insert `key` to
    /// maintain sorted order.
    fun slice_bisect_right<E: store>(self: &Slice<E>, key: u128): u64 {
        let (mut lo, mut hi) = (0, self.keys.length());

        // Invariant: keys[0, lo) <= key < keys[hi, ..)
        while (lo < hi) {
            let mid = (hi - lo) / 2 + lo;
            if (key < self.keys[mid]) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        };

        lo
    }

    /// Insert `key: val` into the slice at ID `slice_id` with depth
    /// `depth`.
    ///
    /// Returns (0, NO_SLICE), if the insertion could be completed
    /// without splitting, otherwise returns the key that was split
    /// upon, and the ID of the new Slice which always sits next to
    /// (and not previously to) `slice_id`.
    ///
    /// Upon returning, sibling pointers are fixed up, but children
    /// pointers will not be.
    ///
    /// Aborts if `key` is already found within the slice.
    fun slice_insert<E: store>(
        self: &mut BigVector<E>,
        slice_id: u64,
        depth: u8,
        key: u128,
        val: E,
    ): (u128, u64) {
        if (depth == 0) {
            self.leaf_insert(slice_id, key, val)
        } else {
            self.node_insert(slice_id, depth - 1, key, val)
        }
    }

    /// Like `slice_insert` but you know that `slice_id` points to a leaf node.
    fun leaf_insert<E: store>(
        self: &mut BigVector<E>,
        slice_id: u64,
        key: u128,
        val: E,
    ): (u128, u64) {
        let leaf: &mut Slice<E> = df::borrow_mut(&mut self.id, slice_id);
        let off = leaf.bisect_left(key);

        if (off < leaf.keys.length() &&
            key == leaf.keys[off]) {
            abort EExists
        };

        // If there is enough space in the current leaf, no need
        // to split.
        if (leaf.keys.length() < self.max_slice_size) {
            leaf.keys.insert(key, off);
            leaf.vals.insert(val, off);
            return (0, NO_SLICE)
        };

        // Split off half the current leaf to be the new `next` leaf.
        let split_at = leaf.vals.length() / 2;
        let mut next = Slice {
            prev: slice_id,
            next: leaf.next,
            keys: leaf.keys.pop_until(split_at),
            vals: leaf.vals.pop_until(split_at),
        };

        // Insert the key-value pair into the correct side of the
        // split -- the first element in the new slice is the pivot.
        //
        // SAFETY: The next slice is guaranteed to be non-empty,
        // because we round down the size of the original slice when
        // splitting, so as long as `leaf.keys` had at least one
        // element at the start of the call, then `next.keys` will
        // have at least one element at this point.
        let pivot = next.keys[0];
        if (key < pivot) {
            leaf.keys.insert(key, off);
            leaf.vals.insert(val, off);
        } else {
            next.keys.insert(key, off - split_at);
            next.vals.insert(val, off - split_at);
        };

        (pivot, self.alloc(next))
    }

    /// Like `slice_insert` but you know that `slice_id` points to an
    /// interior node, and `depth` is the depth of its children, not
    /// itself.
    fun node_insert<E: store>(
        self: &mut BigVector<E>,
        slice_id: u64,
        depth: u8,
        key: u128,
        val: E,
    ): (u128, u64) {
        let node: &mut Slice<u64> = df::borrow_mut(&mut self.id, slice_id);
        let off = node.bisect_right(key);

        let child = node.vals[off];
        let (key, val) = self.slice_insert(child, depth, key, val);

        // The recursive call didn't introduce an extra slice, so no
        // work needed to accommodate it.
        if (val == NO_SLICE) {
            return (0, NO_SLICE)
        };

        // Re-borrow the current node, after the recursive call.
        let node: &mut Slice<u64> = df::borrow_mut(&mut self.id, slice_id);

        // The extra slice can be accommodated in the current node
        // without splitting it.
        if (node.vals.length() < self.max_fan_out) {
            node.keys.insert(key, off);
            node.vals.insert(val, off + 1);
            return (0, NO_SLICE)
        };

        let split_at = node.vals.length() / 2;
        let mut next = Slice {
            prev: slice_id,
            next: node.next,
            keys: node.keys.pop_until(split_at),
            vals: node.vals.pop_until(split_at),
        };

        // SAFETY: `node` is guaranteed to have a key to pop after
        // having `next` split off from it, because:
        //
        //    split_at
        //  = length(node.vals) / 2
        // >= self.max_fan_out  / 2
        // >= MIN_FAN_OUT / 2
        // >= 4 / 2
        //  = 2
        //
        // Meaning there will be at least 2 elements left in the key
        // vector after the split -- one to pop here, and then one to
        // leave behind to ensure the remaining node is at least
        // binary (not vestigial).
        let pivot = node.keys.pop_back();
        if (key < pivot) {
            node.keys.insert(key, off);
            node.vals.insert(val, off + 1);
        } else {
            next.keys.insert(key, off - split_at);
            next.vals.insert(val, off - split_at + 1);
        };

        (pivot, self.alloc(next))
    }

    /// Remove `key` from the slice at ID `slice_id` with depth
    /// `depth`, in `self`.
    ///
    /// `prev_id` and `next_id` are the IDs of slices either side of
    /// `slice_id` that share the same parent, to be used for
    /// redistribution and merging.
    ///
    /// Aborts if `key` does not exist within the slice.
    fun slice_remove<E: store>(
        self: &mut BigVector<E>,
        prev_id: u64,
        prev_key: u128,
        slice_id: u64,
        next_key: u128,
        next_id: u64,
        depth: u8,
        key: u128,
    ): (E, /* RM_FIX */ u8, /* key */ u128) {
        if (depth == 0) {
            self.leaf_remove(
                prev_id,
                prev_key,
                slice_id,
                next_key,
                next_id,
                key,
            )
        } else {
            self.node_remove(
                prev_id,
                prev_key,
                slice_id,
                next_key,
                next_id,
                depth - 1,
                key,
            )
        }
    }

    /// Like `slice_remove` but you know that `slice_id` points to a
    /// leaf.
    fun leaf_remove<E: store>(
        self: &mut BigVector<E>,
        prev_id: u64,
        prev_key: u128,
        slice_id: u64,
        next_key: u128,
        next_id: u64,
        key: u128,
    ): (E, /* RM_FIX */ u8, /* key */ u128) {
        let leaf: &mut Slice<E> = df::borrow_mut(&mut self.id, slice_id);
        let off = leaf.bisect_left(key);

        if (off >= leaf.keys.length()) {
            abort ENotFound
        };

        if (key != leaf.keys[off]) {
            abort ENotFound
        };

        leaf.keys.remove(off);
        let val = leaf.vals.remove(off);

        let remaining = leaf.vals.length();
        let min_slice_size = self.max_slice_size / 2;
        if (remaining >= min_slice_size) {
            return (val, RM_FIX_NOTHING, 0)
        };

        // Try redistribution with a neighbour
        if (prev_id != NO_SLICE) {
            let prev: &Slice<E> = df::borrow(&self.id, prev_id);
            if (prev.vals.length() > min_slice_size) {
                return (
                    val,
                    RM_FIX_STEAL_L,
                    self.slice_redistribute<E, E>(
                        prev_id,
                        prev_key,
                        slice_id,
                    ),
                )
            }
        };

        if (next_id != NO_SLICE) {
            let next: &Slice<E> = df::borrow(&self.id, next_id);
            if (next.vals.length() > min_slice_size) {
                return (
                    val,
                    RM_FIX_STEAL_R,
                    self.slice_redistribute<E, E>(
                        slice_id,
                        next_key,
                        next_id,
                    ),
                )
            }
        };

        // Try merging with a neighbour
        if (prev_id != NO_SLICE) {
            self.slice_merge<E, E>(prev_id, prev_key, slice_id);
            return (val, RM_FIX_MERGE_L, 0)
        };

        if (next_id != NO_SLICE) {
            self.slice_merge<E, E>(slice_id, next_key, next_id);
            return (val, RM_FIX_MERGE_R, 0)
        };

        // Neither neighbour exists, must be the root -- check whether
        // it's empty.
        if (remaining == 0) {
            (val, RM_FIX_EMPTY, 0)
        } else {
            (val, RM_FIX_NOTHING, 0)
        }
    }

    /// Like `slice_remove` but you know that `slice_id` points to an
    /// interior node, and `depth` refers to the depth of its child
    /// nodes.
    fun node_remove<E: store>(
        self: &mut BigVector<E>,
        prev_id: u64,
        prev_key: u128,
        slice_id: u64,
        next_key: u128,
        next_id: u64,
        depth: u8,
        key: u128
    ): (E, /* RM_FIX */ u8, /* key */ u128) {
        let node: &Slice<u64> = df::borrow(&self.id, slice_id);
        let off = node.bisect_right(key);

        let child_id = node.vals[off];

        let (child_prev_id, child_prev_key) = if (off == 0) {
            (NO_SLICE, 0)
        } else (
            node.vals[off - 1],
            node.keys[off - 1],
        );

        let (child_next_id, child_next_key) = if (off == node.keys.length()) {
            (NO_SLICE, 0)
        } else (
            node.vals[off + 1],
            node.keys[off],
        );

        let (val, rm_fix, pivot) = self.slice_remove(
            child_prev_id,
            child_prev_key,
            child_id,
            child_next_key,
            child_next_id,
            depth,
            key,
        );

        // Re-borrow node mutably after recursive call, to perform
        // fix-ups.
        let node: &mut Slice<u64> = df::borrow_mut(&mut self.id, slice_id);

        if (rm_fix == RM_FIX_NOTHING) {
            return (val, RM_FIX_NOTHING, 0)
        } else if (rm_fix == RM_FIX_STEAL_L) {
            *(&mut node.keys[off - 1]) = pivot;
            return (val, RM_FIX_NOTHING, 0)
        } else if (rm_fix == RM_FIX_STEAL_R) {
            *(&mut node.keys[off]) = pivot;
            return (val, RM_FIX_NOTHING, 0)
        } else if (rm_fix == RM_FIX_MERGE_L) {
            node.keys.remove(off - 1);
            node.vals.remove(off);
        } else if (rm_fix == RM_FIX_MERGE_R) {
            node.keys.remove(off);
            node.vals.remove(off + 1);
        } else {
            abort EBadRemove
        };

        let remaining = node.vals.length();
        let min_fan_out = self.max_fan_out / 2;
        if (remaining >= min_fan_out) {
            return (val, RM_FIX_NOTHING, 0)
        };

        // Try redistribution with a neighbour
        if (prev_id != NO_SLICE) {
            let prev: &Slice<u64> = df::borrow(&self.id, prev_id);
            if (prev.vals.length() > min_fan_out) {
                return (
                    val,
                    RM_FIX_STEAL_L,
                    self.slice_redistribute<E, u64>(
                        prev_id,
                        prev_key,
                        slice_id,
                    ),
                )
            }
        };

        if (next_id != NO_SLICE) {
            let next: &Slice<u64> = df::borrow(&self.id, next_id);
            if (next.vals.length() > min_fan_out) {
                return (
                    val,
                    RM_FIX_STEAL_R,
                    self.slice_redistribute<E, u64>(
                        slice_id,
                        next_key,
                        next_id,
                    )
                )
            }
        };

        // Try merging with a neighbour
        if (prev_id != NO_SLICE) {
            self.slice_merge<E, u64>(prev_id, prev_key, slice_id);
            return (val, RM_FIX_MERGE_L, 0)
        };

        if (next_id != NO_SLICE) {
            self.slice_merge<E, u64>(slice_id, next_key, next_id);
            return (val, RM_FIX_MERGE_R, 0)
        };

        // Neither neighbour exists, must be the root. As we are
        // dealing with an interior node, it is considered "empty"
        // when it has only one child (which can replace it), and it
        // is an error for it to be completely empty.
        if (remaining == 0) {
            abort EBadRemove
        } else if (remaining == 1) {
            (val, RM_FIX_EMPTY, 0)
        } else {
            (val, RM_FIX_NOTHING, 0)
        }
    }

    /// Redistribute the elements in `left_id` and `right_id`
    /// separated by `pivot`, evenly between each other. Returns the
    /// new pivot element between the two slices.
    ///
    /// Aborts if left and right are not adjacent slices.
    fun slice_redistribute<E: store, F: store>(
        self: &mut BigVector<E>,
        left_id: u64,
        pivot: u128,
        right_id: u64,
    ): u128 {
        // Remove the slices from `self` to make it easier to
        // manipulate both of them simultaneously.
        let left: Slice<F> = df::remove(&mut self.id, left_id);
        let right: Slice<F> = df::remove(&mut self.id, right_id);

        assert!(left.next == right_id, ENotAdjacent);
        assert!(right.prev == left_id, ENotAdjacent);

        let is_leaf = left.is_leaf();
        let Slice {
            prev: lprev,
            next: lnext,
            keys: mut lkeys,
            vals: mut lvals,
        } = left;

        let Slice {
            prev: rprev,
            next: rnext,
            keys: rkeys,
            vals: rvals,
        } = right;

        let old_l_len = lvals.length();
        let old_r_len = rvals.length();
        let total_len = old_l_len + old_r_len;
        let new_l_len = total_len / 2;
        let new_r_len = total_len - new_l_len;

        // Detect whether the redistribution is left-to-right or right-to-left.
        let left_to_right = if (new_l_len < old_l_len) {
            true
        } else if (new_r_len < old_r_len) {
            false
        } else {
            abort EBadRedistribution
        };

        // Redistribute values
        let (lvals, rvals)  = if (left_to_right) {
            let mut mvals = lvals.pop_until(new_l_len);
            mvals.append(rvals);
            (lvals, mvals)
        } else {
            let mut mvals = rvals;
            let rvals = mvals.pop_n(new_r_len);
            lvals.append(mvals);
            (lvals, rvals)
        };

        // Redistribute keys and move pivot.
        //
        // The pivot moves from the left side to the right side of the
        // middle section depending on whether the keys are from left
        // to right or vice versa.
        //
        // The pivot also changes from inclusive to exclusive based on
        // whether the slices in question are leaves or not.
        //
        // When handling interior nodes, the previous pivot needs to
        // be incorporated during this process.
        let (lkeys, pivot, rkeys) = if (is_leaf && left_to_right) {
            let mut mkeys = lkeys.pop_until(new_l_len);
            let pivot = mkeys[0];
            mkeys.append(rkeys);
            (lkeys, pivot, mkeys)
        } else if (is_leaf && !left_to_right) {
            let mut mkeys = rkeys;
            let rkeys = mkeys.pop_n(new_r_len);
            let pivot = rkeys[0];
            lkeys.append(mkeys);
            (lkeys, pivot, rkeys)
        } else if (!is_leaf && left_to_right) {
            // [left, new-pivot, mid] old-pivot [right]
            // ... becomes ...
            // [left] new-pivot [mid, old-pivot, right]
            let mut mkeys = lkeys.pop_until(new_l_len);
            mkeys.push_back(pivot);
            mkeys.append(rkeys);
            let pivot = lkeys.pop_back();
            (lkeys, pivot, mkeys)
        } else /* !is_leaf && !left_to_right */ {
            // [left] old-pivot [mid, new-pivot, right]
            // ... becomes ...
            // [left, old-pivot, mid] new-pivot [right]
            lkeys.push_back(pivot);
            let mut mkeys = rkeys;
            let rkeys = mkeys.pop_n(new_r_len - 1);
            let pivot = mkeys.pop_back();
            lkeys.append(mkeys);
            (lkeys, pivot, rkeys)
        };

        // Add the slices back to self.
        df::add(&mut self.id, left_id, Slice {
            prev: lprev,
            next: lnext,
            keys: lkeys,
            vals: lvals,
        });

        df::add(&mut self.id, right_id, Slice {
            prev: rprev,
            next: rnext,
            keys: rkeys,
            vals: rvals,
        });

        pivot
    }

    /// Merge the `right_id` slice into `left_id` (represented by
    /// their IDs). Assumes that `left_id` and `right_id` are adjacent
    /// slices, separated by `pivot`, and aborts if this is not the
    /// case.
    ///
    /// Upon success, `left_id` contains all the elements of both
    /// slices, and the `right_id` slice has been removed from the
    /// vector.
    fun slice_merge<E: store, F: store>(
        self: &mut BigVector<E>,
        left_id: u64,
        pivot: u128,
        right_id: u64,
    ) {
        let right: Slice<F> = df::remove(&mut self.id, right_id);
        let left: &mut Slice<F> = df::borrow_mut(&mut self.id, left_id);

        assert!(left.next == right_id, ENotAdjacent);
        assert!(right.prev == left_id, ENotAdjacent);

        if (!left.is_leaf()) {
            left.keys.push_back(pivot);
        };

        let Slice { prev: _, next, keys, vals } = right;
        left.keys.append(keys);
        left.vals.append(vals);

        left.next = next;
        if (next != NO_SLICE) {
            let next: &mut Slice<F> = df::borrow_mut(&mut self.id, next);
            next.prev = left_id;
        }
    }

    // === Test Helpers ===

    #[test_only]
    /// Create a slice just for the purposes of testing bisect functions.
    fun test_slice(keys: vector<u128>): Slice<u64> {
        Slice {
            prev: NO_SLICE,
            next: NO_SLICE,
            keys: keys,
            vals: vector[],
        }
    }

    #[test_only]
    /// Returns the keys from `self`, in pre-order.
    public fun preorder_keys<E: store>(self: &BigVector<E>): vector<vector<u128>> {
        let mut keys = vector[];
        let (slice_id, depth) = (self.root_id, self.depth);
        self.preorder_key_traversal(&mut keys, slice_id, depth);
        keys
    }

    #[test_only]
    fun preorder_key_traversal<E: store>(
        self: &BigVector<E>,
        keys: &mut vector<vector<u128>>,
        slice_id: u64,
        depth: u8,
    ) {
        if (slice_id == NO_SLICE) {
            return
        };

        if (depth == 0) {
            let leaf: &Slice<E> = df::borrow(&self.id, slice_id);
            keys.push_back(leaf.keys);
            return
        };

        let node: &Slice<u64> = df::borrow(&self.id, slice_id);
        keys.push_back(node.keys);

        let mut i = 0;
        while (i < node.vals.length()) {
            let child = node.vals[i];
            self.preorder_key_traversal(keys, child, depth - 1);
            i = i + 1;
        };
    }

    #[test_only]
    /// Returns the values from `self`, in-order.
    public fun inorder_values<E: store + copy>(
        self: &BigVector<E>,
    ): vector<vector<E>> {
        let mut vals = vector[];
        if (self.root_id == NO_SLICE) {
            return vals
        };

        // (1). Traverse intermediate nodes to find left-most leaf.
        let (mut slice_id, mut depth) = (self.root_id, self.depth);
        while (depth > 0) {
            let slice: &Slice<u64> = df::borrow(&self.id, slice_id);
            slice_id = slice.vals[0];
            depth = depth - 1;
        };

        // (2). Iterate through leaves using linked list pointers.
        while (slice_id != NO_SLICE) {
            let leaf: &Slice<E> = df::borrow(&self.id, slice_id);
            vals.push_back(leaf.vals);
            slice_id = leaf.next;
        };

        vals
    }

    // === Tests ===

    #[test]
    fun test_bisect() {
        let slice = test_slice(vector[]);
        assert!(slice.bisect_left(0) == 0, 0);

        let slice = test_slice(vector[1, 3, 5, 7, 9]);
        assert!(slice.bisect_left(0) == 0, 0);
        assert!(slice.bisect_left(1) == 0, 0);
        assert!(slice.bisect_left(2) == 1, 0);
        assert!(slice.bisect_left(3) == 1, 0);
        assert!(slice.bisect_left(4) == 2, 0);
        assert!(slice.bisect_left(5) == 2, 0);
        assert!(slice.bisect_left(6) == 3, 0);
        assert!(slice.bisect_left(7) == 3, 0);
        assert!(slice.bisect_left(8) == 4, 0);
        assert!(slice.bisect_left(9) == 4, 0);
        assert!(slice.bisect_left(10) == 5, 0);
        assert!(slice.bisect_left(11) == 5, 0);

        assert!(slice.bisect_right(0) == 0, 0);
        assert!(slice.bisect_right(1) == 1, 0);
        assert!(slice.bisect_right(2) == 1, 0);
        assert!(slice.bisect_right(3) == 2, 0);
        assert!(slice.bisect_right(4) == 2, 0);
        assert!(slice.bisect_right(5) == 3, 0);
        assert!(slice.bisect_right(6) == 3, 0);
        assert!(slice.bisect_right(7) == 4, 0);
        assert!(slice.bisect_right(8) == 4, 0);
        assert!(slice.bisect_right(9) == 5, 0);
        assert!(slice.bisect_right(10) == 5, 0);
        assert!(slice.bisect_right(11) == 5, 0);
    }
}
