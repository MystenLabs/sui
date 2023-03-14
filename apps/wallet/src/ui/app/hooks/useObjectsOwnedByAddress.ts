// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

import type { SuiAddress } from '@mysten/sui.js';

export function useObjectsOwnedByAddress(address?: SuiAddress | null) {
    const rpc = useRpcClient();
    return useQuery(
        ['objects-owned', address],
        () => rpc.getObjectsOwnedByAddress({ owner: address! }),
        {
            enabled: !!address,
        }
    );
}
