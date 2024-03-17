// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GraphQLQueryOptions } from '@mysten/graphql-transport';
import { ResultOf, VariablesOf, graphql } from '@mysten/sui.js/dist/cjs/graphql/schemas/2024-01';
import { SuiGraphQLClient, GraphQLDocument } from '@mysten/sui.js/graphql';

export interface PageInfo {
	hasNextPage: boolean;
	hasPreviousPage: boolean;
	endCursor: string | null;
	startCursor: string | null;
}

export type BenchmarkParams = {
	paginateForwards: boolean,
	limit: number,
	numPages: number
}

export type PaginationParams = {
	first?: number,
	after?: string,
	last?: number,
	before?: string
}

export class PaginationV2 {
	paginateForwards: boolean;
	limit: number;
	cursor?: string;

	constructor(
	  paginateForwards: boolean,
	  limit: number,
	) {
	  this.paginateForwards = paginateForwards;
	  this.limit = limit;
	}

	getParams(): PaginationParams {
		if (this.paginateForwards) {
			return {
				first: this.limit,
				after: this.cursor
			}
		}
		return {
			last: this.limit,
			before: this.cursor
		}
	}

	getCursor(): string | undefined {
		return this.cursor;
	}

	setCursor(cursor: string) {
		this.cursor = cursor;
	}
}


// TODO doc comments once stabilized
/// Caller is responsible for providing a `testFn` that returns the `PageInfo` for the benchmark to
/// paginate through.
export async function benchmark_connection_query<T extends Record<string, GraphQLDocument>>(
	client: SuiGraphQLClient<T>,
	paginateForwards: boolean,
	testFn: (client: SuiGraphQLClient<T>, cursor: string | null) => Promise<PageInfo | undefined>,
	pages: number = 10,
	runParallel: boolean = false
): Promise<number[]> {
	let cursors: Array<string> = [];
	let hasNextPage = true;
	let cursor: string | null = null;
    let durations: number[] = [];

	for (let i = 0; i < pages && hasNextPage; i++) {
		const start = performance.now();
		const result = await testFn(client, cursor);
		const duration = performance.now() - start;
        durations.push(duration);

		// TODO: this is a bit awkward because we can't tell if we timed out ...
		// Simple way is just to consider all that exceed the timeout as a timeout
		if (result == undefined) {
			return durations;
		}

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

			return result;
		});

		await Promise.all(fetchFutures);
	}

    return durations;
}


export async function v2<Q extends Record<string, GraphQLDocument>>(
	client: SuiGraphQLClient<Q>,
	benchmarkParams: BenchmarkParams,
	testFn: (client: SuiGraphQLClient<Q>, cursor: PaginationParams) => Promise<PageInfo | undefined>,
): Promise<number[]> {
	let { paginateForwards, limit, numPages } = benchmarkParams;

	let cursors: Array<string> = [];
	let hasNextPage = true;
	let cursor: string | null = null;
    let durations: number[] = [];

	let pagination = new PaginationV2(paginateForwards, limit);

	for (let i = 0; i < numPages && hasNextPage; i++) {
		const start = performance.now();
		const result = await testFn(client, pagination.getParams());
		const duration = performance.now() - start;
        durations.push(duration);

		// TODO: this is a bit awkward because we can't tell if we timed out ...
		// Simple way is just to consider all that exceed the timeout as a timeout
		if (result == undefined) {
			return durations;
		}

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

	// we should report the cursors here, not in the calls
	return durations;
};


interface PaginationVariables {
	before?: string;
	after?: string;
	first?: number;
	last?: number;
  }

type Metrics = {
	min: number,
	p50: number,
	p90: number,
	p95: number,
	mean: number,
	max: number
}

export function metrics(durations: number[]): Metrics {
    const sorted = durations.sort((a, b) => a - b);
    const p50 = sorted[Math.floor(durations.length * 0.5)];
    const p90 = sorted[Math.floor(durations.length * 0.9)];
    const p95 = sorted[Math.floor(durations.length * 0.95)];
	const sum = sorted.reduce((a, b) => a + b, 0);
    return {
		min: sorted[0],
        p50,
        p90,
        p95,
		mean: sum / durations.length,
        max: sorted[sorted.length - 1]
    };
}

export function report<T>(params: T, cursors: string[], metrics: Metrics) {
    let reportObject;

    if (metrics.min > 5000) {
        reportObject = {
            status: "TIMED OUT",
            params,
            cursors
        };
    } else {
        reportObject = {
            status: "COMPLETED",
            params,
            cursors,
            metrics
        };
    }

    const jsonReport = JSON.stringify(reportObject, null, 2);
    console.log(jsonReport);
}

/*
todo: can consider something like this:
export async function benchmark_connection_query<T extends Record<string, GraphQLDocument>, M extends (...args: any[]) => any>(
    client: SuiGraphQLClient<T>,
    paginateForwards: boolean,
    testFn: (client: SuiGraphQLClient<T>, cursor: string | null) => Promise<PageInfo>,
    metricsFn: M,
    pages: number = 10,
    runParallel: boolean = false
): Promise<ReturnType<M>> {
    // ...
}
*/

// todo how to handle pagination? can we simplify this more broadly as just another set of params to
// provide?
