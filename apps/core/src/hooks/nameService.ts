// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useRpcClient } from '../api/RpcClientContext';

export function useResolveSuiNSAddress(name?: string | null) {
    const rpc = useRpcClient();

    return useQuery(
        ['resolve-suins-address', name],
        async () => {
            return await rpc.resolveNameServiceAddress({
                name: name!,
            });
        },
        {
            enabled: !!name,
            refetchOnWindowFocus: false,
            refetchOnMount: false,
            retry: false,
        }
    );
}

export function useResolveSuiNSName(address?: SuiAddress | null) {
    const rpc = useRpcClient();

    return useQuery(
        ['resolve-suins-name', address],
        async () => {
            // NOTE: We only fetch 1 here because it's the default name.
            const { data } = await rpc.resolveNameServiceNames({
                address: address!,
                limit: 1,
            });

            return data[0] || null;
        },
        {
            enabled: !!address,
            refetchOnWindowFocus: false,
            refetchOnMount: false,
            retry: false,
        }
    );
}
