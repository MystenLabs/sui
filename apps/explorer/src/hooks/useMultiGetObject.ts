// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export function useMultiGetObjects(ids: string[]) {
    const rpc = useRpcClient();
    return useQuery(
        ['multi-objects', ids],
        () => {
            if (!ids.length) {
                return [];
            }
            return rpc.multiGetObjects({
                ids,
                options: {
                    showType: true,
                    showContent: true,
                    showOwner: true,
                },
            });
        },

        { enabled: !!ids }
    );
}
