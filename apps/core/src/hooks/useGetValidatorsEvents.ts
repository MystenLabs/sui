// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useSuiClient } from '@mysten/dapp-kit';
import { SuiEvent, type EventId } from '@mysten/sui/client';
import { useQuery } from '@tanstack/react-query';

type GetValidatorsEvent = {
	limit: number | null;
	order: 'ascending' | 'descending';
};

// NOTE: This copys the query limit from our Rust JSON RPC backend, this needs to be kept in sync!
const QUERY_MAX_RESULT_LIMIT = 50;
const VALIDATORS_EVENTS_QUERY = '0x3::validator_set::ValidatorEpochInfoEventV2';

//TODO: get validatorEvents by validator address
export function useGetValidatorsEvents({ limit, order }: GetValidatorsEvent) {
	const client = useSuiClient();
	// Since we are getting events based on the number of validators, we need to make sure that the limit
	// is not null and cache by the limit number of validators can change from network to network
	return useQuery({
		queryKey: ['validatorEvents', limit, order],
		queryFn: async () => {
			if (!limit) {
				// Do some validation at the runtime level for some extra type-safety
				// https://tkdodo.eu/blog/react-query-and-type-script#type-safety-with-the-enabled-option
				throw new Error(
					`Limit needs to always be defined and non-zero! Received ${limit} instead.`,
				);
			}

			if (limit > QUERY_MAX_RESULT_LIMIT) {
				let hasNextPage = true;
				let currCursor: EventId | null | undefined;
				const results: SuiEvent[] = [];

				while (hasNextPage && results.length < limit) {
					const validatorEventsResponse = await client.queryEvents({
						query: { MoveEventType: VALIDATORS_EVENTS_QUERY },
						cursor: currCursor,
						limit: Math.min(limit, QUERY_MAX_RESULT_LIMIT),
						order,
					});

					hasNextPage = validatorEventsResponse.hasNextPage;
					currCursor = validatorEventsResponse.nextCursor;
					results.push(...(validatorEventsResponse.data as SuiEvent[]));
				}
				return results.slice(0, limit);
			}

			const validatorEventsResponse = await client.queryEvents({
				query: { MoveEventType: VALIDATORS_EVENTS_QUERY },
				limit,
				order,
			});
			return validatorEventsResponse.data;
		},
		enabled: !!limit,
	});
}
