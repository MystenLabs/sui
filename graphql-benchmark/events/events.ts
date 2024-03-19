// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiGraphQLClient } from '@mysten/sui.js/graphql';
import { print } from 'graphql';
import { VariablesOf } from '@mysten/sui.js/graphql/schemas/2024-01';

import { benchmark_connection_query, PageInfo } from '../benchmark';
import { queries } from './queries';
import { EventFilterParameters, generateFilters } from './parameterize';

const client = new SuiGraphQLClient({
	url: 'http://127.0.0.1:8000',
	queries,
});

type Variables = VariablesOf<(typeof queries)['queryEvents']>;

async function queryEvents(
	client: SuiGraphQLClient<typeof queries>,
	variables: Variables,
): Promise<{ pageInfo: PageInfo | undefined; variables: Variables }> {
	let response = await client.execute('queryEvents', { variables });
	let data = response.data;
	return {
		pageInfo: data?.events.pageInfo,
		variables,
	};
}


import fs from 'fs';
import path from 'path';

// Get the JSON file path from the command line arguments
const jsonFilePath = process.argv[2];

// Read the JSON file
const jsonData = fs.readFileSync(path.resolve(__dirname, jsonFilePath), 'utf-8');

// Parse the JSON data
const data: EventFilterParameters = JSON.parse(jsonData);

// Rest of your code...

/// Orchestrates test suite
async function eventsSuite(client: SuiGraphQLClient<typeof queries>) {
	let limit = 50;
	let numPages = 10;

	// parameterize
	console.log(print(queries['queryEvents']));
	for (let paginateForwards of [true, false]) {
		for (let filter of generateFilters(data)) {
			await benchmark_connection_query(
				{ paginateForwards, limit, numPages },
				async (paginationParams) => {
					let new_variables: Variables = {
						...paginationParams,
						filter,
					};
					return await queryEvents(client, new_variables);
				},
			);
		}
	}
}

eventsSuite(client);

// npx ts-node events/events.ts parameters.json
