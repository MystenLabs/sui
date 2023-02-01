// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type PaginatedEvents,
    type EventQuery,
    type EventId,
} from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from './useRpc';

type GetEventsProps = {
    query: EventQuery;
    cursor?: EventId | null;
    limit: number | null;
    order: 'ascending' | 'descending';
};

export function useGetEvents({
    query,
    cursor,
    limit,
    order,
}: GetEventsProps): UseQueryResult<PaginatedEvents> {
    const rpc = useRpc();
    const eventCursor = cursor || null;
    const eventLimit = limit || null;

    const response = useQuery(
        ['events', query],
        () => rpc.getEvents(query, eventCursor, eventLimit, order),
        { enabled: !!limit }
    );

    return response;
}
