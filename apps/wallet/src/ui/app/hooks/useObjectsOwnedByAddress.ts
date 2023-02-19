// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useRpc } from '_hooks';

import type { SuiAddress } from '@mysten/sui.js';

export function useObjectsOwnedByAddress(address?: SuiAddress | null) {
    const rpc = useRpc();
    return useQuery(
        ['objects-owned', address],
        () => rpc.getObjectsOwnedByAddress(address!),
        { enabled: !!address }
    );
}
