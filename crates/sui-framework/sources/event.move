// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::event {
    /// An attachable event. Should be emitted once unpacked from a
    /// a wrapper. Holds any custom event data inside.
    struct Event<T: copy + drop + store> has store {
        data: T
    }

    /// Create a new `Event` wrapper for custom `data`.
    public fun create<T: copy + drop + store>(data: T): Event<T> {
        Event { data }
    }

    /// Unpack contents of `Event` and `emit` them.
    public fun emit_event<T: copy + drop + store>(evt: Event<T>) {
        let Event { data } = evt;
        emit(data)
    }

    /// Add `t` to the event log of this transaction
    // TODO(https://github.com/MystenLabs/sui/issues/19):
    // restrict to internal types once we can express this in the ability system
    public native fun emit<T: copy + drop>(event: T);
}
