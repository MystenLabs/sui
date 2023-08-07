#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Generates ../data/coin_factory/sources/coin_factory.move to create NUM_VARIANTS of a function returning an N-tuple of coins

import os

NUM_VARIANTS = 64

factory_file = os.path.join(os.path.dirname(__file__), os.pardir, "data", "coin_factory", "sources", "coin_factory.move")

with open(factory_file, 'w') as f:
    f.write("// Copyright (c) Mysten Labs, Inc.\n")
    f.write("// SPDX-License-Identifier: Apache-2.0\n")

    f.write("module coiner::coin_factory {\n")
    f.write("    use std::option;\n")
    f.write("    use sui::coin::{Self, Coin, TreasuryCap};\n")
    f.write("    use sui::transfer;\n")
    f.write("    use std::vector;\n");
    f.write("    use sui::tx_context::{Self, TxContext};\n")
    f.write("\n")
    f.write("    struct COIN_FACTORY has drop {}\n")
    f.write("\n")
    f.write("    fun init(witness: COIN_FACTORY, ctx: &mut TxContext) {\n")
    f.write("        let (treasury, metadata) = coin::create_currency(witness, 6, b\"COIN_FACTORY\", b\"\", b\"\", option::none(), ctx);\n")
    f.write("        // this actually does not make any sense in real life (metadata should actually be frozen)\n")
    f.write("        // but makes it easier to find package object in effects (as it will be the only immutable\n")
    f.write("        // object if metadata is not frozen)\n")
    f.write("        transfer::public_share_object(metadata);\n")
    f.write("        transfer::public_transfer(treasury, tx_context::sender(ctx))\n")
    f.write("    }\n")
    f.write("\n");
    f.write("    public fun mint_vec(\n");
    f.write("        cap: &mut TreasuryCap<COIN_FACTORY>,\n");
    f.write("        value: u64,\n");
    f.write("        size: u64,\n");
    f.write("        ctx: &mut TxContext\n");
    f.write("    ): vector<Coin<COIN_FACTORY>> {\n");
    f.write("        let v = vector::empty<Coin<COIN_FACTORY>>();\n");
    f.write("        let i = 0;\n");
    f.write("        while (i < size) {\n");
    f.write("            vector::push_back(&mut v, coin::mint(cap, value, ctx));\n");
    f.write("            i = i + 1;\n");
    f.write("        };\n");
    f.write("        v\n");
    f.write("    }\n\n");


    for i in range(1, NUM_VARIANTS + 1):
        f.write("    public fun unpack_" + str(i) +"(v: &mut vector<Coin<COIN_FACTORY>>): (\n")
        f.write(',\n'.join([x for x in ["         Coin<COIN_FACTORY>"] for j in range(i)]))
        f.write("\n    ) {\n")
        f.write("        (\n")
        f.write(',\n'.join([x for x in ["         vector::pop_back(v)"] for j in range(i)]))
        f.write("\n        )\n")
        f.write("    }\n\n")

    f.write("}\n")
