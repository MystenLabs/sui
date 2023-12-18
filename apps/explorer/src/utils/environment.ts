// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Environment, Network, RecordSource, Store } from 'relay-runtime';

// Define a function that fetches the results of an operation (query/mutation/etc)
// and returns its results as a Promise:
function fetchQuery(operation, variables, cacheConfig, uploadables) {
	return fetch('/graphql', {
		method: 'POST',
		headers: {
			// Add authentication and other headers here
			'content-type': 'application/json',
		},
		body: JSON.stringify({
			query: operation.text, // GraphQL text from input
			variables,
		}),
	}).then((response) => response.json());
}

// Create a network layer from the fetch function
const network = Network.create(fetchQuery);
const store = new Store(new RecordSource());

const environment = new Environment({
	network,
	store,
	// ... other options
});

export default environment;
