// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

export function useGetOwnedObjects(address?: SuiAddress | null) {
    const rpc = useRpcClient();
    return useQuery(
        ['get-owned-objects', address],
        async () =>
            await rpc.getOwnedObjects({
                owner: address!,
                options: {
                    showType: true,
                    showContent: true,
                    showDisplay: true,
                },
            }),
        { enabled: !!address }
    );
}
