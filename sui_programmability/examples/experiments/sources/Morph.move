// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Experiments::Morph {
    use Sui::ID::{Self, VersionedID};
    use Sui::TxContext::{Self, TxContext};

    struct Inner has store { value: u64 }
    struct Wrapper has key { id: VersionedID, inner: Inner }

    public fun empty(): Inner {
        Inner { value: 0 }
    }

    public fun wrap(inner: Inner, ctx: &mut TxContext): Wrapper {
        Wrapper { id: TxContext::new_id(ctx), inner }
    }

    public fun unwrap(wrapper: Wrapper): Inner {
        let Wrapper { id, inner } = wrapper;
        ID::delete(id);
        inner
    }

    public fun destroy(inner: Inner): u64 {
        let Inner { value } = inner;
        value
    }
}

#[test_only]
module Experiments::MorphTests {
    use Experiments::Morth;
    use Sui::TestScenario::{Self, ctx};

    #[test]
    fun test_morphing() {
        let test = &mut TestScenario::begin(&@0xF1FA);

        let inner = Morth::empty();
        let outer = Morth::wrap(inner, ctx(test));
        let inne
    }
}
