// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export interface PageInfo {
	hasNextPage: boolean;
	hasPreviousPage: boolean;
	endCursor: string | null;
	startCursor: string | null;
}

export type BenchmarkParams = {
	paginateForwards: boolean;
	limit: number;
	numPages: number;
};

export type PaginationParams = {
	first?: number;
	after?: string;
	last?: number;
	before?: string;
};

export class PaginationV2 {
	paginateForwards: boolean;
	limit: number;
	cursor?: string;

	constructor(paginateForwards: boolean, limit: number) {
		this.paginateForwards = paginateForwards;
		this.limit = limit;
	}

	getParams(): PaginationParams {
		if (this.paginateForwards) {
			return {
				first: this.limit,
				after: this.cursor,
			};
		}
		return {
			last: this.limit,
			before: this.cursor,
		};
	}

	getCursor(): string | undefined {
		return this.cursor;
	}

	setCursor(pageInfo: PageInfo) {
		if (this.paginateForwards) {
			if (pageInfo.hasNextPage) {
				this.cursor = pageInfo.endCursor!;
			}
		} else {
			if (pageInfo.hasPreviousPage) {
				this.cursor = pageInfo.startCursor!;
			}
		}
	}
}


/// Caller is responsible for providing a `testFn` that returns the `PageInfo` for the benchmark to
/// paginate through.
export async function benchmark_connection_query(
	benchmarkParams: BenchmarkParams,
	testFn: (
		cursor: PaginationParams,
	) => Promise<{ pageInfo: PageInfo | undefined; variables: any }>,
): Promise<number[]> {
	let { paginateForwards, limit, numPages } = benchmarkParams;

	const cursors: Array<string> = [];
	let hasNextPage = true;
	let durations: number[] = [];

	let pagination = new PaginationV2(paginateForwards, limit);
	let queryParams;

	for (let i = 0; i < numPages && hasNextPage; i++) {
		let start = performance.now();
		let { pageInfo: result, variables } = await testFn(pagination.getParams());
		let duration = performance.now() - start;
		durations.push(duration);
		if (i == 0) {
			queryParams = variables;
		}

		// TODO: this is a bit awkward because we can't tell if we timed out ...
		// Simple way is just to consider all that exceed the timeout as a timeout
		if (result == undefined) {
			break;
		}

		let cursor = pagination.getCursor();
		if (cursor) {
			cursors.push(cursor);
		}

		// Defer to pagination to update cursor
		pagination.setCursor(result);
	}

	// sleep for 1 second
	await new Promise((r) => setTimeout(r, 1000));

	report(queryParams, cursors, metrics(durations));
	return durations;
}

type Metrics = {
	min: number;
	p50: number;
	p90: number;
	p95: number;
	mean: number;
	max: number;
};

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
		max: sorted[sorted.length - 1],
	};
}

type Report = {
	params: any;
	cursors: string[];
	status: 'COMPLETED' | 'TIMED OUT';
	metrics?: Metrics;
};

export function report<T>(params: T, cursors: string[], metrics: Metrics) {
	// Set defaults and shared data
	let reportObject: Report = {
		status: 'COMPLETED',
		params,
		cursors,
	};

	if (metrics.min > 5000) {
		reportObject.status = 'TIMED OUT';
	} else {
		reportObject.metrics = metrics;
	}

	const jsonReport = JSON.stringify(reportObject, null, 2);
	console.log(jsonReport);
}
