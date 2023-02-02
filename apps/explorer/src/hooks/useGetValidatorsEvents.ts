// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type PaginatedEvents, type EventId } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from './useRpc';

export const VALIDATORS_EVENTS_QUERY = '0x2::validator_set::ValidatorEpochInfo';

type GetValidatorsEvent = {
    cursor?: EventId | null;
    limit: number | null;
    order: 'ascending' | 'descending';
};

//TODO: get validatorEvents by validator address
export function useGetValidatorsEvents({
    cursor,
    limit,
    order,
}: GetValidatorsEvent): UseQueryResult<PaginatedEvents> {
    const rpc = useRpc();
    const eventCursor = cursor || null;
    const eventLimit = limit || null;

    // since we are getting events base on the number of validators, we need to make sure that the limit is not null and cache by the limit
    // number of validators can change from network to network
    const response = useQuery(
        ['validatorEvents', limit],
        () =>
            rpc.getEvents(
                { MoveEvent: VALIDATORS_EVENTS_QUERY },
                eventCursor,
                eventLimit,
                order
            ),
        { enabled: !!limit }
    );

    return response;
}
