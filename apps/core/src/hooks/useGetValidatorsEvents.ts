// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { type EventId } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

export const VALIDATORS_EVENTS_QUERY =
    '0x3::validator_set::ValidatorEpochInfoEvent';

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
}: GetValidatorsEvent) {
    const rpc = useRpcClient();
    const eventCursor = cursor || null;
    const eventLimit = limit || null;

    // since we are getting events base on the number of validators, we need to make sure that the limit is not null and cache by the limit
    // number of validators can change from network to network
    return useQuery(
        ['validatorEvents', limit],
        () =>
            rpc.queryEvents({
                query: { MoveEventType: VALIDATORS_EVENTS_QUERY },
                cursor: eventCursor?.txDigest,
                limit: eventLimit,
                order,
            }),
        { enabled: !!limit }
    );
}
