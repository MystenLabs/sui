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


let eventPackages = ['0x2d6733a32e957430324196dc5d786d7c839f3c7bbfd92b83c469448b988413b1', '0x2', '0x3'];
let eventModules = ['coin_flip', 'display', 'validator'];
let eventTypes = ['Outcome', 'DisplayCreated', 'StakingRequestEvent'];
let senders = ['0x18889a72b0cd8072196ee4cf269a16d8e559c69088f7488ecb76de002f088010', // from last 0x2
'0x8eab656650ded2b5e1a2577ad102202595a361b375ba953769c192f30d59fc4c'
];
let emittingPackages = ['0x7f6ce7ade63857c4fd16ef7783fed2dfc4d7fb7e40615abdb653030b76aef0c6', '0x549e8b69270defbfafd4f94e17ec44cdbdd99820b33bda2278dea3b9a32d3f55'];
let emittingModules = ['staked_sui_vault', 'native_pool'];


type Variables = VariablesOf<typeof queries['queryEvents']>;

async function events(client: SuiGraphQLClient<typeof queries>, benchmarkParams: BenchmarkParams, variables: Variables) {
    await benchmark_connection_query(client, benchmarkParams, async (client, paginationParams) => {
        return await eventsHelper(client,
            {
				...paginationParams,
				...variables,
			}
        );
    }).catch ((error) => {
        console.error(error);
        return [];
    });
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

function* emitTypes(packages: string[], modules: string[], types: string[]) {
    for (let [i, package_] of packages.entries()) {
        yield* emitModulesHelper(package_, modules[i]);
        yield* emitTypesHelper(package_, modules[i], types[i]);
    }
}

function* emitModules(packages: string[], modules: string[]) {
	for (let [i, package_] of packages.entries()) {
		yield* emitModulesHelper(package_, modules[i]);
	}
}

function* emitModulesHelper(eventPackage: string, module: string) {
    yield eventPackage;
    yield eventPackage + '::' + module;
}

function* emitTypesHelper(eventPackage: string, module: string, type: string) {
	yield* emitModulesHelper(eventPackage, module);
    yield eventPackage + '::' + module + '::' + type;
}

type Filter = { eventType?: string, sender?: string, emittingModule?: string };

function* generateFilters(eventPackages: string[], eventModules: string[], eventTypes: string[], senders: string[], emittingPackages: string[], emittingModules: string[]): Generator<Filter> {
	// generate filters with one field
  	for (let eventType of emitTypes(eventPackages, eventModules, eventTypes)) {
    	yield { eventType };
  	}
  	for (let sender of senders) {
		yield { sender };
  	}
  	for (let emittingModule of emitModules(emittingPackages, emittingModules)) {
		yield { emittingModule };
  	}

  	// generate filters with two fields
	for (let eventType of emitTypes(eventPackages, eventModules, eventTypes)) {
		for (let sender of senders) {
			yield { eventType, sender };
		}
		for (let emittingModule of emitModules(emittingPackages, emittingModules)) {
			yield { eventType, emittingModule };
		}
	}
	for (let sender of senders) {
		for (let emittingModule of emitModules(emittingPackages, emittingModules)) {
			yield { sender, emittingModule };
		}
	}

	// generate filters with three fields
	for (let eventType of emitTypes(eventPackages, eventModules, eventTypes)) {
		for (let sender of senders) {
			for (let emittingModule of emitModules(emittingPackages, emittingModules)) {
				yield { eventType, sender, emittingModule };
			}
		}
	}
}


async function eventsSuite(client: SuiGraphQLClient<typeof queries>) {
  let limit = 50;
  let numPages = 10;

  for (let filter of generateFilters(eventPackages, eventModules, eventTypes, senders, emittingPackages, emittingModules)) {
    await events(client, { paginateForwards: true, limit, numPages }, { filter });
    await events(client, { paginateForwards: false, limit, numPages }, { filter });
  }
}

async function eventsSuiteOld(client: SuiGraphQLClient<typeof queries>) {
	let limit = 50;
	let numPages = 10;
	let paginateForwards = true;

	// eventType
	// for (let eventType of emitTypes(eventPackages, eventModules, eventTypes)) {
		// await events(client, { paginateForwards, limit, numPages }, {
			// filter: { eventType }});
		// await events(client, { paginateForwards: false, limit, numPages }, { filter: { eventType } });
	// }

	// eventType, sender
	// for (let eventType of emitTypesHelper(eventPackages[2], eventModules[2], eventTypes[2])) {
		// for (let sender of senders) {
			// await events(client, { paginateForwards, limit, numPages }, { filter: { eventType, sender }});
			// await events(client, { paginateForwards: false, limit, numPages }, { filter: { eventType, sender }});
		// }
	// }

	// eventType, emittingModule
	// for (let eventType of emitTypesHelper(eventPackages[1], eventModules[1], eventTypes[1])) {
		// for (let emittingModule of emitModules(emittingPackages, emittingModules)) {
			// await events(client, { paginateForwards, limit, numPages }, { filter: { eventType, emittingModule }});
			// await events(client, { paginateForwards: false, limit, numPages }, { filter: { eventType, emittingModule }});
		// }
	// }

	// eventType, sender, emittingModule
	for (let eventType of emitTypesHelper(eventPackages[2], eventModules[2], eventTypes[2])) {
		for (let sender of senders) {
			for (let emittingModule of emitModules(emittingPackages, emittingModules)) {
				await events(client, { paginateForwards, limit, numPages }, { filter: { eventType, sender, emittingModule }});
				await events(client, { paginateForwards: false, limit, numPages }, { filter: { eventType, sender, emittingModule }});
			}
		}
	}
}

eventsSuite(client);


// how do we combine eventType with emittingModule, sender, transactionDigest?
// either need to collect, or have prepopulated data
