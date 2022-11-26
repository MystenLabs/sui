// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Encapsulates CapyAdmin functionality allowing for faster
/// txs and lower gas fees for new capy / item batches.
module capy::capy_admin {
    use capy::capy::{Self, CapyManagerCap, CapyRegistry, Capy};
    use capy::capy_market::{Self, CapyMarket};
    use capy::capy_items::{Self, CapyItem};
    use sui::tx_context::TxContext;
    use sui::bcs;

    entry fun add_gene(
        cap: &CapyManagerCap,
        reg: &mut CapyRegistry,
        name: vector<u8>,
        definitions: vector<vector<u8>>,
        ctx: &mut TxContext
    ) {
        capy::add_gene(cap, reg, name, definitions, ctx);
    }

    entry fun batch_sell_capys(
        cap: &CapyManagerCap,
        reg: &mut CapyRegistry,
        market: &mut CapyMarket<Capy>,
        genes: vector<vector<u8>>,
        price: u64,
        ctx: &mut TxContext
    ) {
        let capys = capy::batch(cap, reg, genes, ctx);
        capy_market::batch_list(market, capys, price, ctx);
    }

    entry fun batch_sell_items(
        cap: &CapyManagerCap,
        market: &mut CapyMarket<CapyItem>,
        data: vector<u8>,
        ctx: &mut TxContext
    ) {
        let bcs = bcs::new(data);
        let len = bcs::peel_vec_length(&mut bcs);
        while (len > 0) {
            let (type, name, price, q) = (
                bcs::peel_vec_u8(&mut bcs),
                bcs::peel_vec_u8(&mut bcs),
                bcs::peel_u64(&mut bcs),
                bcs::peel_u8(&mut bcs)
            );

            while (q > 0) {
                let item = capy_items::create_item(cap, type, name, ctx);
                capy_market::list(market, item, price, ctx);
                q = q - 1;
            };

            len = len - 1;
        };
    }
}
