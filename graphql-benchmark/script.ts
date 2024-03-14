// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGraphQLClient } from '@mysten/sui.js/graphql';
import { graphql, readFragment } from '@mysten/sui.js/graphql/schemas/2024-01';

import { CoinConnectionData, CoinsQuery, PageInfoFragment } from './coins';
import { EventsQuery } from './events';

const queries = {
	getCoins: CoinsQuery,
	queryEvents: EventsQuery,
};

interface PageInfo {
	hasNextPage: boolean;
	hasPreviousPage: boolean;
	endCursor: string | null;
	startCursor: string | null;
}

async function benchmark(
	client: SuiGraphQLClient<typeof queries>,
	paginateForwards: boolean,
	testFn: (client: SuiGraphQLClient<typeof queries>, cursor: string | null) => Promise<PageInfo>,
	pages: number = 10,
	runParallel: boolean = false
): Promise<void> {
	let cursors: Array<string> = [];
	let hasNextPage = true;
	let cursor: string | null = null;

	for (let i = 0; i < pages && hasNextPage; i++) {
		const start = performance.now();
		const result = await testFn(client, cursor);
		const duration = performance.now() - start;

		console.log(`Time to fetch page: ${duration}ms`);

		if (paginateForwards) {
			hasNextPage = result.hasNextPage;
			cursor = result.endCursor;
			if (result.endCursor) {
				cursors.push(result.endCursor);
			}
		} else {
			hasNextPage = result.hasPreviousPage;
			cursor = result.startCursor;
			if (result.startCursor) {
				cursors.push(result.startCursor);
			}
		}
	}

	// Artificial delay to simulate processing
	await new Promise((resolve) => setTimeout(resolve, 1000));

	// Run tests in parallel
	if (runParallel) {
		const fetchFutures = cursors.map(async (cursor) => {
			const start = performance.now();
			const result = await testFn(client, cursor);
			const duration = performance.now() - start;

			console.log(`Benchmark duration: ${duration}ms`);
			return result;
		});

		await Promise.all(fetchFutures);
	}
}

const client = new SuiGraphQLClient<typeof queries>({
	url: 'http://127.0.0.1:8000',
	queries,
});

async function runBenchmarksSequentially(client: SuiGraphQLClient<typeof queries>) {
	let limit = 50;
	console.log('getCoins');
	await benchmark(client, true, async (client, cursor) => {
		const response = await client.execute('getCoins', {
			variables: {
				first: limit,
				after: cursor,
			},
		});
		const data = response.data!;
		const pageInfoFragment = readFragment(CoinConnectionData, data.coins).pageInfo;
		const pageInfo = readFragment(PageInfoFragment, pageInfoFragment);
		return pageInfo;
	}).catch(console.error);

	console.log('getCoins with showContent');
	await benchmark(client, true, async (client, cursor) => {
		const response = await client.execute('getCoins', {
			variables: {
				first: limit,
				after: cursor,
				showContent: true,
			},
		});
		const data = response.data!;
		const pageInfoFragment = readFragment(CoinConnectionData, data.coins).pageInfo;
		const pageInfo = readFragment(PageInfoFragment, pageInfoFragment);
		return pageInfo;
	}).catch(console.error);

	console.log('getCoins with showPreviousTransaction');
	await benchmark(client, true, async (client, cursor) => {
		const response = await client.execute('getCoins', {
			variables: {
				first: limit,
				after: cursor,
				showPreviousTransaction: true,
			},
		});
		const data = response.data!;
		const pageInfoFragment = readFragment(CoinConnectionData, data.coins).pageInfo;
		const pageInfo = readFragment(PageInfoFragment, pageInfoFragment);
		return pageInfo;
	}).catch(console.error);

	console.log('getCoins with showContent and showPreviousTransaction');
	await benchmark(client, true, async (client, cursor) => {
		const response = await client.execute('getCoins', {
			variables: {
				first: limit,
				after: cursor,
				showContent: true,
				showPreviousTransaction: true,
			},
		});
		const data = response.data!;
		const pageInfoFragment = readFragment(CoinConnectionData, data.coins).pageInfo;
		const pageInfo = readFragment(PageInfoFragment, pageInfoFragment);
		return pageInfo;
	}).catch(console.error);
}

// runBenchmarksSequentially(client);

async function runEventsBenchmarks(client: SuiGraphQLClient<typeof queries>) {
	let limit = 50;
	let senders = new Set<string>();
	let emittingModules = new Set<string>();
	// let eventType = '0x3::validator::StakingRequestEvent';
	let eventType = '0x3::validator';
	// let eventType = '0x2d6733a32e957430324196dc5d786d7c839f3c7bbfd92b83c469448b988413b1::coin_flip::Outcome';
	/*
	starting with eventType, the possible combinations are:
	1. eventType
	2. eventType+sender
	3. eventType+emittingModule
	4. eventType+transactionDigest - I feel like this one should be pretty performant. well hmm, we don't have an index on transaction_digest, so maybe not ...
	5. eventType+sender+emittingModule
	6. eventType+sender+transactionDigest
	7. eventType+emittingModule+transactionDigest
	8. eventType+sender+emittingModule+transactionDigest
	*/

	// hypothesis: if the initial filter is efficient, then combinations of it is fine and we don't need composite indexes

	// collect senders: each page will yield up to 100 senders
	console.log('query on eventType=' + eventType + ' and collect senders');
	await benchmark(client, true, async (client, cursor) => {
		let limit = 50;
		const response = await client.execute('queryEvents', {
			variables: {
				first: limit,
				after: cursor,
				filter: {
					eventType
				},
			},
		});
		const data = response.data!;
		// collect senders
		data.events.nodes.forEach((node: any) => {
			if (node.sender) senders.add(node.sender.address);
		});
		const pageInfo = data.events.pageInfo;
		return pageInfo;
	}, 100).catch(console.error);

	console.log("how many senders: " + senders.size);

	// select 10 random senders
	let test_senders = Array.from(senders).slice(0, 10);
    for (const sender of test_senders) {
        console.log('query on eventType=' + eventType + ' and sender=' + sender);
        try {
            await benchmark(client, true, async (client, cursor) => {
                const response = await client.execute('queryEvents', {
                    variables: {
                        first: limit,
                        after: cursor,
                        filter: {
                            eventType,
                            sender
                        }
                    },
                });
				const data = response.data!;
				const pageInfo = data.events.pageInfo;
				return pageInfo;
            });
        } catch (error) {
            console.error(error);
        }
    }

	// now we need to collect some emittingModules
	// each page will yield up to 100 emittingModules
	console.log('refetch query on eventType=' + eventType + ' and collect emittingModules');
	console.log("these queries will be slower since emittingModule/sendingModule is an N+1 query");
	await benchmark(client, true, async (client, cursor) => {
		const response = await client.execute('queryEvents', {
			variables: {
				first: limit,
				after: cursor,
				filter: {
					eventType,
				},
				showSendingModule: true
			},
		});
		const data = response.data!;
		// collect senders
		data.events.nodes.forEach((node: any) => {
			if (node.sendingModule) {
				let package_ = node.sendingModule.package.address;
				let name = node.sendingModule.name;
				// add the package and package::name to test fetching by broad and more specific
				emittingModules.add(package_);
				emittingModules.add(package_ + '::' + name);
			}
		});
		const pageInfo = data.events.pageInfo;
		return pageInfo;
	}, 2).catch(console.error);


	// use the first 100 emittingModules
	let test_modules = Array.from(emittingModules).slice(0, 10);
	for (const module of test_modules) {
		console.log('query on eventType=' + eventType + ' and emittingModule=' + module);
		try {
			await benchmark(client, true, async (client, cursor) => {
				const response = await client.execute('queryEvents', {
					variables: {
						first: limit,
						after: cursor,
						filter: {
							eventType,
							emittingModule: module
						}
					},
				});
				const data = response.data!;
				const pageInfo = data.events.pageInfo;
				return pageInfo;
			});
		} catch (error) {
			console.error(error);
		}
	}


	// what about transactionDigest

	// and then we can just use random combinations from sender, emittingModule, transactionDigest

	// what's left after eventType? starting from sender:
	// 1. sender
	// 2. sender+emittingModule
	// 3. sender+transactionDigest
	// 4. sender+emittingModule+transactionDigest

	// then emittingModule:
	// 1. emittingModule
	// 2. emittingModule+transactionDigest

	// then transactionDigest:
	// 1. transactionDigest

	// what's left?
	// 1. none

}

runEventsBenchmarks(client);
