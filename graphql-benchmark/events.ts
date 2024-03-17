// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { graphql, ResultOf, VariablesOf } from '@mysten/sui.js/graphql/schemas/2024-01';
import { PageInfo, BenchmarkParams, benchmark_connection_query, metrics, report} from './benchmark';
import { SuiGraphQLClient } from '@mysten/sui.js/graphql';

// with cursor
// by sender
// by transaction digest
// by emitting module
// by event type

/*
Combinations
Choosing 1 Item: 4 combinations

sender
tx_digest
emittingModule
eventType
Choosing 2 Items: 6 combinations

sender, tx_digest
sender, emittingModule
sender, eventType
tx_digest, emittingModule
tx_digest, eventType
emittingModule, eventType
Choosing 3 Items: 4 combinations

sender, tx_digest, emittingModule
sender, tx_digest, eventType
sender, emittingModule, eventType
tx_digest, emittingModule, eventType
Choosing All 4 Items: 1 combination

sender, tx_digest, emittingModule, eventType
Choosing None: 1 combination (though not a filtering scenario, it represents querying without any specific filter).


And note that these will always be bounded by a checkpoint_sequence_number, and potentially tx_sequence_number + event_sequence_number
*/

export const EventsQuery = graphql(`
query queryEvents(
	$filter: EventFilter!
	$before: String
	$after: String
	$first: Int
	$last: Int
    $showSendingModule: Boolean = false
    $showContents: Boolean = false
) {
	events(filter: $filter, first: $first, after: $after, last: $last, before: $before) {
		pageInfo {
			hasNextPage
			hasPreviousPage
			endCursor
			startCursor
		}
		nodes {
            sendingModule @include(if: $showSendingModule){
                package {
                    address
                }
                name
            }
            sender {
                address
            }
            type {
                repr
            }
            json @include(if: $showContents)
		}
	}
}`);

const queries = { queryEvents: EventsQuery };

const client = new SuiGraphQLClient({
	url: 'http://127.0.0.1:8000',
	queries
});


let packages = ['0x2d6733a32e957430324196dc5d786d7c839f3c7bbfd92b83c469448b988413b1', '0x2', '0x3'];
let modules = ['coin_flip', 'display', 'validator'];
let types = ['Outcome', 'DisplayCreated', 'StakingRequestEvent'];
let senders = ['0x18889a72b0cd8072196ee4cf269a16d8e559c69088f7488ecb76de002f088010', // from last 0x2
'0x8eab656650ded2b5e1a2577ad102202595a361b375ba953769c192f30d59fc4c'
];

type EventFilter = VariablesOf<typeof queries['queryEvents']>['filter'];
type Variables = VariablesOf<typeof queries['queryEvents']>;

async function events(client: SuiGraphQLClient<typeof queries>, benchmarkParams: BenchmarkParams, filter: EventFilter) {
    let durations = await benchmark_connection_query(client, benchmarkParams, async (client, paginationParams) => {
        return await eventsHelper(client,
            {
				...paginationParams,
				filter,
			}
        );
    }).catch ((error) => {
        console.error(error);
        return [];
    });

	report(filter, [], metrics(durations));
}

// TODO: ideally benchmark can execute, and just rely on the helper to extract pageInfo
// benchmark needs helper to be able to extract pageInfo
async function eventsHelper(client: SuiGraphQLClient<typeof queries>, variables: Variables): Promise<{ pageInfo: PageInfo | undefined, variables: Variables }>{
	let response = await client.execute('queryEvents', { variables });
	let data = response.data;
	return {
		pageInfo: data?.events.pageInfo,
		variables
	};
}

function* emitEventTypes() {
	for (let [i, package_] of packages.entries()) {
		yield* emitEventTypesHelper(package_, modules[i], types[i]);
	}
}

function* emitEventTypesHelper(package_: string, module: string, type: string) {
	yield package_;
	yield package_ + '::' + module;
	yield package_ + '::' + module + '::' + type;
}

async function eventsSuite(client: SuiGraphQLClient<typeof queries>) {
	let limit = 50;
	let numPages = 10;
	let paginateForwards = true;

	// for (let eventType of emitEventTypes()) {
		// await events(client, { paginateForwards, limit, numPages }, { eventType });
		// await events(client, { paginateForwards: false, limit, numPages }, { eventType });
	// }

	for (let eventType of emitEventTypesHelper(packages[2], modules[2], types[2])) {
		for (let sender of senders) {
			await events(client, { paginateForwards, limit, numPages }, { eventType, sender });
			await events(client, { paginateForwards: false, limit, numPages }, { eventType, sender });
		}
	}

}

eventsSuite(client);


// how do we combine eventType with emittingModule, sender, transactionDigest?
// either need to collect, or have prepopulated data
