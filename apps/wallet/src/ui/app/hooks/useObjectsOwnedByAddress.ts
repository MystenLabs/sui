// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { useRpc } from '_hooks';

import type { SuiAddress, SuiObjectInfo } from '@mysten/sui.js';

export function useObjectsOwnedByAddress(
    address: SuiAddress | null
): UseQueryResult<SuiObjectInfo[], Error> {
    const rpc = useRpc();
    return useQuery(
        ['objects-owned', address],
        () => rpc.getObjectsOwnedByAddress(address!),
        { enabled: !!address }
    );
}
