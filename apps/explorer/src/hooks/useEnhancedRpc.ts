// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useRpcClient } from '@mysten/core';
import { Connection, JsonRpcProvider } from '@mysten/sui.js';
import { useMemo } from 'react';

import { useNetwork } from './useNetwork';

import { Network } from '~/utils/api/DefaultRpcClient';

// TODO: Use enhanced RPC locally by default
export function useEnhancedRpcClient() {
    const [network] = useNetwork();
    const rpc = useRpcClient();
    const enhancedRpc = useMemo(() => {
        if (network === Network.LOCAL) {
            return new JsonRpcProvider(
                new Connection({ fullnode: 'http://localhost:9124' })
            );
        }

        return rpc;
    }, [network, rpc]);

    return enhancedRpc;
}
