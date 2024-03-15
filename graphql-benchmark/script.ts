// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGraphQLClient } from '@mysten/sui.js/graphql';
import { graphql, readFragment } from '@mysten/sui.js/graphql/schemas/2024-01';

import { CoinConnectionData, CoinsQuery, PageInfoFragment } from './coins';
import { EventsQuery } from './events';
import { benchmark_connection_query } from './benchmark';

const queries = {
	getCoins: CoinsQuery,
	queryEvents: EventsQuery,
	objectPreviousTxBlock: graphql(`
	query objectPreviousTxBlock($first: Int $last: Int $before: String $after: String) {
		objects(first: $first, last: $last, before: $before, after: $after) {
		pageInfo {
		  startCursor
		  endCursor
		  hasNextPage
		  hasPreviousPage
		}
		nodes {
		  previousTransactionBlock {
			digest
		  }
		}
	  }
	}
`)
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
	await benchmark_connection_query(client, true, async (client, cursor) => {
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



async function yeet(client: SuiGraphQLClient<typeof queries>) {
	let limit = 50;
	console.log('object previous tx blocks');
	await benchmark_connection_query(client, true, async (client, cursor) => {
		let limit = 50;
		// todo: i feel like we could have benchmark manage this
		const response = await client.execute('objectPreviousTxBlock', {
			variables: {
				first: limit,
				after: cursor,
			},
		});
		const data = response.data!;
		const pageInfo = data.objects.pageInfo;
		return pageInfo;
	}, 100).catch(console.error);
}

yeet(client);
