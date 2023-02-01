// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type PaginatedEvents, type EventId } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from './useRpc';

import { VALIDATORS_EVENTS_QUERY } from '~/pages/validator/ValidatorDataTypes';

type GetValidatorsEvent = {
    cursor?: EventId | null;
    limit: number | null;
    order: 'ascending' | 'descending';
};

export function useGetValidatorsEvents({
    cursor,
    limit,
    order,
}: GetValidatorsEvent): UseQueryResult<PaginatedEvents> {
    const rpc = useRpc();
    const eventCursor = cursor || null;
    const eventLimit = limit || null;

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
