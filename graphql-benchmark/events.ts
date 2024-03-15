// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { graphql, ResultOf, VariablesOf } from '@mysten/sui.js/graphql/schemas/2024-01';
import { Pagination, benchmark_connection_query, metrics, report } from './benchmark';
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
	await benchmark_connection_query(client, true, async (client, cursor) => {
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
            await benchmark_connection_query(client, true, async (client, cursor) => {
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
	await benchmark_connection_query(client, true, async (client, cursor) => {
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
			await benchmark_connection_query(client, true, async (client, cursor) => {
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

let packages = ['0x2d6733a32e957430324196dc5d786d7c839f3c7bbfd92b83c469448b988413b1', '0x2', '0x3'];
let modules = ['coin_flip', 'display', 'validator'];
let types = ['Outcome', 'DisplayCreated', 'StakingRequestEvent'];
let senders = ['0x18889a72b0cd8072196ee4cf269a16d8e559c69088f7488ecb76de002f088010', // from last 0x2
'0x8eab656650ded2b5e1a2577ad102202595a361b375ba953769c192f30d59fc4c'
];

type EventFilter = VariablesOf<typeof queries['queryEvents']>['filter'];
type Result = ResultOf<typeof queries['queryEvents']>;


async function events(client: SuiGraphQLClient<typeof queries>, pagination: Pagination, filter: EventFilter) {
	let { paginateForwards, limit, numPages } = pagination;
	let initialVariables = {
		...(paginateForwards ? { first: limit } : { last: limit })
		, filter
	};
	let cursors: string[] = [];

    let durations = await benchmark_connection_query(client, paginateForwards, async (client, cursor) => {
		// todo this is kind of awkward - overlapping ownership
		if (cursor)
		cursors.push(cursor);
        const response = await client.execute('queryEvents', {
            variables: {
				...initialVariables,
				...(paginateForwards ? { after: cursor } : { before: cursor }),
			}
        });
        const data = response.data;
        const pageInfo = data?.events.pageInfo;
        return pageInfo;
    }, numPages).catch ((error) => {
        console.error(error);
        return [];
    });

	report(JSON.stringify(initialVariables, null, 2), cursors, metrics(durations));
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

	for (let eventType of emitEventTypes()) {
		await events(client, { paginateForwards, limit, numPages }, { eventType });
		await events(client, { paginateForwards: false, limit, numPages }, { eventType });
	}

	// for (let eventType of emitEventTypesHelper(packages[2], modules[2], types[2])) {
		// for (let sender of senders) {
			// await events(client, { paginateForwards, limit, numPages }, { eventType, sender });
			// await events(client, { paginateForwards: false, limit, numPages }, { eventType, sender });
		// }
	// }

}

eventsSuite(client);


// how do we combine eventType with emittingModule, sender, transactionDigest?
// either need to collect, or have prepopulated data
