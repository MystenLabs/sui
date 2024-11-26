// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs, BcsType } from '@mysten/sui/bcs';
import { SuiClient } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';
import pLimit from 'p-limit';

const limit = pLimit(5);

const Order = bcs.struct('Order', {
	order_id: bcs.u64(),
	client_order_id: bcs.u64(),
	price: bcs.u64(),
	original_quantity: bcs.u64(),
	quantity: bcs.u64(),
	is_bid: bcs.bool(),
	owner: bcs.Address,
	expire_timestamp: bcs.u64(),
	self_matching_prevention: bcs.u8(),
});

const Table = bcs.struct('Table', {
	id: bcs.Address,
	size: bcs.u64(),
});

function Field<K extends BcsType<any>, V extends BcsType<any>>(K: K, V: V) {
	return bcs.struct('Field<K,V>', {
		id: bcs.Address,
		name: K,
		value: V,
	});
}

const LinkedTable = bcs.struct('LinkedTable', {
	id: bcs.Address,
	size: bcs.u64(),
	head: bcs.vector(bcs.u64()),
	tail: bcs.vector(bcs.u64()),
});

const TickLevel = bcs.struct('TickLevel', {
	price: bcs.u64(),
	open_orders: LinkedTable,
});

function Node<V extends BcsType<any>>(V: V) {
	return bcs.struct('Node<V>', {
		prev: bcs.vector(bcs.u64()),
		next: bcs.vector(bcs.u64()),
		value: V,
	});
}

function Leaf<V extends BcsType<any>>(V: V) {
	return bcs.struct('Leaf<V>', {
		key: bcs.u64(),
		value: V,
		parent: bcs.u64(),
	});
}

const CritbitTree = bcs.struct('CritbitTree', {
	root: bcs.u64(),
	internal_nodes: Table,
	leaves: Table,
	min_leaf: bcs.u64(),
	max_leaf: bcs.u64(),
	next_internal_node_index: bcs.u64(),
	next_leaf_index: bcs.u64(),
});

const Pool = bcs.struct('Pool', {
	id: bcs.Address,
	bids: CritbitTree,
	asks: CritbitTree,
	next_bid_order_id: bcs.u64(),
	next_ask_order_id: bcs.u64(),
	usr_open_orders: Table,
	taker_fee_rate: bcs.u64(),
	maker_rebate_rate: bcs.u64(),
	tick_size: bcs.u64(),
	lot_size: bcs.u64(),
});

const PoolCreated = bcs.struct('PoolCreated', {
	pool_id: bcs.Address,
	base_asset: bcs.string(),
	quote_asset: bcs.string(),
	// We don't need other fields for the mev bot
});

// Create a client connected to the Sui network
const client = new SuiClient({ url: 'https://sui-mainnet.mystenlabs.com/json-rpc' });

// Retrieve all DeepBook pools using the PoolCreated events
let allPools = await retrieveAllPools();

// Retrieve all expired orders from each pool
let allExpiredOrdersPromises = [];
for (let pool of allPools) {
	allExpiredOrdersPromises.push(
		retrieveExpiredOrders(pool.pool_id).then((expiredOrders) => {
			return { pool, expiredOrders };
		}),
	);
}
let allExpiredOrders = (await Promise.all(allExpiredOrdersPromises)).flat();

// Create a transaction to clean up all expired orders and get the estimated storage fee rebate using devInspectTransactionBlock
let { rebate } = await createCleanUpTransaction(allExpiredOrders);

console.log(`Total estimated storage fee rebate: ${rebate / 1e9} SUI`);

// Implementer Todo : sign and execute the transaction

async function retrieveAllPools() {
	let page = await client.queryEvents({ query: { MoveEventType: '0xdee9::clob_v2::PoolCreated' } });
	let data = page.data;
	while (page.hasNextPage) {
		page = await client.queryEvents({
			query: {
				MoveEventType: '0xdee9::clob_v2::PoolCreated',
			},
			cursor: page.nextCursor,
		});
		data.push(...page.data);
	}

	return data.map((event) => {
		try {
			return PoolCreated.fromBase64(event.bcs);
		} catch (err) {
			console.error("Failed to parse event:", err, "Event:", event);
			return null;
		}
	}).filter((pool) => pool !== null);
}

async function retrieveExpiredOrders(poolId: string) {
	let pool = await client.getObject({ id: poolId, options: { showBcs: true } });
	let poolData = pool.data?.bcs!;

	switch (poolData.dataType) {
		// Pool is a move object
		case 'moveObject': {
			let pool = Pool.fromBase64(poolData.bcsBytes);
			let asks = await getAllDFPages(pool.asks.leaves.id);
			let bids = await getAllDFPages(pool.bids.leaves.id);

			let ids = [...bids, ...asks].map((bid) => bid.objectId);
			let tickLevels = [];

			for (let chunk of chunks(ids, 50)) {
				tickLevels.push(
					...(await client
						.multiGetObjects({ ids: chunk, options: { showBcs: true } })
						.then((responses) => {
							return responses.map((response) => {
								if (!response.error) {
									let tickLevelBcs = response.data?.bcs!;
									switch (tickLevelBcs.dataType) {
										case 'moveObject': {
											return Field(bcs.u64(), Leaf(TickLevel)).fromBase64(tickLevelBcs.bcsBytes)
												.value.value;
										}
									}
								} else {
									// An object could be deleted during query, ignore
								}
							});
						})),
				);
			}

			let orderIdsPromises = [];
			for (let tickLevel of tickLevels.filter((tickLevel) => tickLevel !== undefined)) {
				// Restrict concurrent requests to avoid a rate limit issue on a public Full node
				orderIdsPromises.push(
					limit(() =>
						getAllDFPages(tickLevel!.open_orders.id).then((data) =>
							data.map((node) => node.objectId),
						),
					),
				);
			}
			let orderIds = (await Promise.all(orderIdsPromises)).flat();
			let orders = await getOrders(orderIds);
			let expiredOrders = orders.filter(
				(order) => order && Number(order.expire_timestamp) <= Date.now(),
			);
			console.log(
				`Pool ${poolId} has ${expiredOrders?.length} expired orders out of ${orders?.length} orders`,
			);
			return expiredOrders;
		}
	}
	throw new Error('Invalid pool data type');
}

async function createCleanUpTransaction(poolOrders: { pool: any; expiredOrders: any[] }[]) {
	let tx = new Transaction();

	for (let poolOrder of poolOrders) {
		let orderIds = poolOrder.expiredOrders.map((order) => tx.object(order.order_id));
		let orderOwners = poolOrder.expiredOrders.map((order) => tx.pure.address(order.owner));

		let orderIdVec = tx.makeMoveVec({ elements: orderIds, type: 'u64' });
		let orderOwnerVec = tx.makeMoveVec({ elements: orderOwners, type: 'address' });

		tx.moveCall({
			target: `0xdee9::clob_v2::clean_up_expired_orders`,
			arguments: [tx.object(poolOrder.pool.pool_id), tx.object('0x6'), orderIdVec, orderOwnerVec],
			typeArguments: [poolOrder.pool.base_asset, poolOrder.pool.quote_asset],
		});
	}
	let result = await client.devInspectTransactionBlock({
		transactionBlock: tx,
		sender: '0xbab1ae46252d520bb8d82e6d8f2b83acb9c1c4226944516b4c6c45b0d00ef17d',
	});

	let costSummary = result.effects.gasUsed;
	let rebate =
		parseInt(costSummary.storageRebate) -
		parseInt(costSummary.storageCost) -
		parseInt(costSummary.computationCost);

	return { rebate, tx };
}

// Helper functions to retrieve all pages of dynamic fields
async function getAllDFPages(parentId: string) {
	let page = await client.getDynamicFields({
		parentId: parentId,
	});
	let data = page.data;
	while (page.hasNextPage) {
		page = await client.getDynamicFields({
			parentId: parentId,
			cursor: page.nextCursor,
		});
		data.push(...page.data);
	}
	return data.filter((node) => node.objectId !== undefined);
}

async function getOrders(ids: string[]) {
	let result = [];
	for (let chunk of chunks(ids, 50)) {
		result.push(
			...(await client
				.multiGetObjects({
					ids: chunk,
					options: { showBcs: true },
				})
				.then((responses) => {
					return responses.map((response) => {
						if (!response.error) {
							let objBCS = response.data?.bcs!;
							switch (objBCS.dataType) {
								case 'moveObject': {
									return Field(bcs.u64(), Node(Order)).fromBase64(objBCS.bcsBytes).value.value;
								}
							}
						} else {
							// An object could be deleted during query, ignore
						}
					});
				})),
		);
	}
	return result.filter((order) => order !== undefined);
}

// Helper function to split an array into chunks
function chunks(data: any[], size: number) {
	return Array.from(new Array(Math.ceil(data.length / size)), (_, i) =>
		data.slice(i * size, i * size + size),
	);
}
