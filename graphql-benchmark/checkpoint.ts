// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiGraphQLClient } from '@mysten/sui.js/graphql';
import { graphql } from '@mysten/sui.js/graphql/schemas/2024-01';

import { benchmark_connection_query, BenchmarkParams, metrics } from './benchmark';

export const SingleCheckpoint = graphql(`
	query SingleCheckpoint($digest: String, $seqNum: Int) {
		checkpoint(id: { digest: $digest, sequenceNumber: $seqNum }) {
			sequenceNumber
		}
	}
`);

export const EpochCheckpoints = graphql(`
	query EpochCheckpoints($epochId: Int, $first: Int, $after: String, $last: Int, $before: String) {
		epoch(id: $epochId) {
			checkpoints(first: $first, after: $after, last: $last, before: $before) {
				pageInfo {
					startCursor
					endCursor
					hasNextPage
					hasPreviousPage
				}
				nodes {
					sequenceNumber
				}
			}
		}
	}
`);

export const Checkpoints = graphql(`
	query Checkpoints($first: Int, $after: String, $last: Int, $before: String) {
		checkpoints(first: $first, after: $after, last: $last, before: $before) {
			pageInfo {
				startCursor
				endCursor
				hasNextPage
				hasPreviousPage
			}
			nodes {
				sequenceNumber
			}
		}
	}
`);

// TODO: can we share function params?
// TODO: how can we combine queries together? For example, if I want to run 50 `SingleCheckpoint` in a single graphql request

export const queries = {
	EpochCheckpoints,
	Checkpoints,
};

const client = new SuiGraphQLClient({
	url: 'http://127.0.0.1:8000',
	queries,
});

async function checkpoints(
	client: SuiGraphQLClient<typeof queries>,
	benchmarkParams: BenchmarkParams,
) {
	let durations = await benchmark_connection_query(
		client,
		benchmarkParams,
		async (client, paginationParams) => {
			let variables = {
				...paginationParams,
			};
			const response = await client.execute('Checkpoints', {
				variables,
			});
			const data = response.data;
			const pageInfo = data?.checkpoints.pageInfo;
			return {
				pageInfo,
				variables,
			};
		},
	).catch((error) => {
		console.error(error);
		return [];
	});

	console.log(metrics(durations));
}

async function epochCheckpoints(
	client: SuiGraphQLClient<typeof queries>,
	benchmarkParams: BenchmarkParams,
	epochId: number | null,
) {
	let durations = await benchmark_connection_query(
		client,
		benchmarkParams,
		async (client, paginationParams) => {
			let variables = {
				...paginationParams,
				epochId,
			};
			const response = await client.execute('EpochCheckpoints', {
				variables,
			});
			const data = response.data;
			const pageInfo = data?.epoch!.checkpoints.pageInfo;
			return {
				pageInfo,
				variables,
			};
		},
	).catch((error) => {
		console.error(error);
		return [];
	});

	console.log(metrics(durations));
}

async function checkpointSuite(client: SuiGraphQLClient<typeof queries>) {
	let paginateForwards = true;

	let benchmarkParams = {
		paginateForwards,
		limit: 50,
		numPages: 10,
	};

	await checkpoints(client, benchmarkParams);
	await checkpoints(client, { ...benchmarkParams, paginateForwards: false });

	let epochId = 320;
	await epochCheckpoints(client, benchmarkParams, epochId);
	await epochCheckpoints(client, { ...benchmarkParams, paginateForwards: false }, epochId);
}

checkpointSuite(client);
