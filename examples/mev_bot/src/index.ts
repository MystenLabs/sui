// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {SuiClient} from '@mysten/sui.js/client';
import {BCS, fromB58, fromB64, getSuiMoveConfig} from "@mysten/bcs";
import {TransactionBlock} from "@mysten/sui.js/transactions";
import pLimit from "p-limit";
const limit = pLimit(5);

let bcs = new BCS(getSuiMoveConfig());
bcs.registerStructType("Order", {
    order_id: "u64",
    client_order_id: "u64",
    price: "u64",
    original_quantity: "u64",
    quantity: "u64",
    is_bid: "bool",
    owner: "address",
    expire_timestamp: "u64",
    self_matching_prevention: "u8"
});

bcs.registerStructType("Table", {
    id: "address",
    size: "u64",
});

bcs.registerStructType("Field<K,V>", {
    id: "address",
    name: "K",
    value: "V",
});

bcs.registerStructType("TickLevel", {
    price: "u64",
    open_orders: "LinkedTable",
});

bcs.registerStructType("LinkedTable", {
    id: "address",
    size: "u64",
    head: "vector<u64>",
    tail: "vector<u64>",
});

bcs.registerStructType("Node<V>", {
    prev: "vector<u64>",
    next: "vector<u64>",
    value: "V"
});

bcs.registerStructType("Leaf<V>", {
    key: "u64",
    value: "V",
    parent: "u64",
});

bcs.registerStructType("CritbitTree", {
    root: "u64",
    internal_nodes: "Table",
    leaves: "Table",
    min_leaf: "u64",
    max_leaf: "u64",
    next_internal_node_index: "u64",
    next_leaf_index: "u64"
});

bcs.registerStructType("Pool", {
    id: "address",
    bids: "CritbitTree",
    asks: "CritbitTree",
    next_bid_order_id: "u64",
    next_ask_order_id: "u64",
    usr_open_orders: "Table",
    taker_fee_rate: "u64",
    maker_rebate_rate: "u64",
    tick_size: "u64",
    lot_size: "u64"
});

bcs.registerStructType("PoolCreated", {
    pool_id: "address",
    base_asset: "string",
    quote_asset: "string",
    // We don't need other fields for the mev bot
});

// Create a client connected to the Sui network
const client = new SuiClient({url: "https://explorer-rpc.mainnet.sui.io:443"});

// Retrieve all DeepBook pools using the PoolCreated events
let allPools = await retrieveAllPools();

// Retrieve all expired orders from each pool
let allExpiredOrdersPromises = [];
for (let pool of allPools) {
    allExpiredOrdersPromises.push(retrieveExpiredOrders(pool.pool_id).then((expiredOrders) => {
        return {pool, expiredOrders}
    }));
}
let allExpiredOrders = (await Promise.all(allExpiredOrdersPromises)).flat();

// Create a transaction to clean up all expired orders and get the estimated storage fee rebate using devInspectTransactionBlock
let {rebate, tx} = await createCleanUpTransaction(allExpiredOrders);

console.log(`Total estimated storage fee rebate: ${rebate / 1e9} SUI`);

// Implementer Todo : sign and execute the transaction

async function retrieveAllPools() {
    let page = await client.queryEvents({query: {MoveEventType: "0xdee9::clob_v2::PoolCreated"}});
    let data = page.data;
    while (page.hasNextPage) {
        page = await client.queryEvents({
            query: {
                MoveEventType: "0xdee9::clob_v2::PoolCreated"
            },
            cursor: page.nextCursor
        });
        data.push(...page.data);
    }
    return data.map((event) => {
        return bcs.de("PoolCreated", fromB58(event.bcs))
    });
}

async function retrieveExpiredOrders(poolId: string) {
    let pool = await client.getObject({id: poolId, options: {showBcs: true}})
    let poolData = pool.data?.bcs!;

    switch (poolData.dataType) {
        // Pool is a move object
        case "moveObject": {
            let pool = bcs.de("Pool", fromB64(poolData.bcsBytes));
            let asks = await getAllDFPages(pool.asks.leaves.id);
            let bids = await getAllDFPages(pool.bids.leaves.id);

            let ids = [...bids, ...asks].map((bid) => bid.objectId);
            let tickLevels = [];

            for (let chunk of chunks(ids, 50)) {
                tickLevels.push(...await client.multiGetObjects({ids: chunk, options: {showBcs: true}})
                    .then((responses) => {
                        return responses.map((response) => {
                            if (!response.error) {
                                let tickLevelBcs = response.data?.bcs!;
                                switch (tickLevelBcs.dataType) {
                                    case "moveObject": {
                                        return bcs.de("Field<u64, Leaf<TickLevel>>", fromB64(tickLevelBcs.bcsBytes)).value.value;
                                    }
                                }
                            } else {
                                // An object could be deleted during query, ignore
                            }
                        })
                    }))
            }

            let orderIdsPromises = [];
            for (let tickLevel of tickLevels.filter((tickLevel) => tickLevel !== undefined)) {
                // Restrict concurrent requests to avoid a rate limit issue on a public Full node
                orderIdsPromises.push(limit(() => getAllDFPages(tickLevel.open_orders.id)
                    .then((data) => data.map((node) => node.objectId)))
                );
            }
            let orderIds = (await Promise.all(orderIdsPromises)).flat();
            let orders = await getOrders(orderIds);
            let expiredOrders = orders.filter((order) => order.expire_timestamp <= Date.now());
            console.log(`Pool ${poolId} has ${expiredOrders?.length} expired orders out of ${orders?.length} orders`);
            return expiredOrders;
        }
    }
    throw new Error("Invalid pool data type");
}

async function createCleanUpTransaction(poolOrders: { pool: any, expiredOrders: any[] }[]) {
    let tx = new TransactionBlock();

    for (let poolOrder of poolOrders) {
        let orderIds = poolOrder.expiredOrders.map((order) => tx.pure(order.order_id, BCS.U64));
        let orderOwners = poolOrder.expiredOrders.map((order) => tx.pure(order.owner, BCS.ADDRESS));

        let orderIdVec = tx.makeMoveVec({objects: orderIds, type: "u64"});
        let orderOwnerVec = tx.makeMoveVec({objects: orderOwners, type: "address"});

        tx.moveCall({
            target: `0xdee9::clob_v2::clean_up_expired_orders`,
            arguments: [
                tx.object(poolOrder.pool.pool_id),
                tx.object("0x6"),
                orderIdVec,
                orderOwnerVec
            ],
            typeArguments: [poolOrder.pool.base_asset, poolOrder.pool.quote_asset]
        });
    }
    let result = await client.devInspectTransactionBlock({
        transactionBlock: tx,
        sender: "0xbab1ae46252d520bb8d82e6d8f2b83acb9c1c4226944516b4c6c45b0d00ef17d"
    });

    let costSummary = result.effects.gasUsed;
    let rebate = parseInt(costSummary.storageRebate) - parseInt(costSummary.storageCost) - parseInt(costSummary.computationCost);

    return {rebate, tx};
}

// Helper functions to retrieve all pages of dynamic fields
async function getAllDFPages(parentId: string) {
    let page = await client.getDynamicFields({
        parentId: parentId
    });
    let data = page.data;
    while (page.hasNextPage) {
        page = await client.getDynamicFields({
            parentId: parentId,
            cursor: page.nextCursor
        });
        data.push(...page.data);
    }
    return data.filter((node) => node.objectId !== undefined);
}

async function getOrders(ids: string[]) {
    let result = [];
    for (let chunk of chunks(ids, 50)) {
        result.push(...await client.multiGetObjects({
            ids: chunk,
            options: {showBcs: true}
        }).then((responses) => {
            return responses.map((response) => {
                if (!response.error) {
                    let objBCS = response.data?.bcs!;
                    switch (objBCS.dataType) {
                        case "moveObject": {
                            let order = bcs.de("Field<u64, Node<Order>>", fromB64(objBCS.bcsBytes));
                            return order.value.value;
                        }
                    }
                } else {
                    // An object could be deleted during query, ignore
                }
            })
        }))
    }
    return result.filter((order) => order !== undefined);
}

// Helper function to split an array into chunks
function chunks(data: any[], size: number) {
    return Array.from(
        new Array(Math.ceil(data.length / size)),
        (_, i) => data.slice(i * size, i * size + size)
    );
}
