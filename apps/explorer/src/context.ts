// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    createContext,
    useEffect,
    useLayoutEffect,
    useState,
    type Dispatch,
    type SetStateAction,
} from 'react';
import { useSearchParams } from 'react-router-dom';

import { CURRENT_ENV } from './utils/envUtil';

import type { Network } from './utils/api/DefaultRpcClient';

const LOCALSTORE_RPC_KEY = CURRENT_ENV + 'sui-explorer-rpc';
const LOCALSTORE_RPC_TIME_KEY = CURRENT_ENV + 'sui-explorer-rpc-lastset';
// Below is 3 hours in milliseconds:
const LOCALSTORE_RPC_VALID_MS = 60000 * 60 * 3;

export const NetworkContext = createContext<
    [Network | string, Dispatch<SetStateAction<Network | string>>]
>(['', () => null]);

const wasNetworkSetLongTimeAgo = (): boolean => {
    const lastEpoch = Number(
        window.localStorage.getItem(LOCALSTORE_RPC_TIME_KEY)
    );

    const nowEpoch = Date.now().valueOf();

    if (nowEpoch - lastEpoch >= LOCALSTORE_RPC_VALID_MS) {
        window.localStorage.setItem(
            LOCALSTORE_RPC_TIME_KEY,
            nowEpoch.toString()
        );
        return true;
    } else {
        return false;
    }
};

export function useNetwork(): [
    string,
    Dispatch<SetStateAction<Network | string>>
] {
    const [searchParams] = useSearchParams();
    const [network, setNetwork] = useState<Network | string>(() => {
        const storedNetwork = window.localStorage.getItem(LOCALSTORE_RPC_KEY);
        if (!storedNetwork || wasNetworkSetLongTimeAgo()) {
            return CURRENT_ENV;
        }
        return storedNetwork;
    });

    useLayoutEffect(() => {
        const rpcUrl = searchParams.get('rpcUrl');
        if (rpcUrl) {
            setNetwork(rpcUrl);
        }
    }, [searchParams]);

    useEffect(() => {
        // If network in UI changes, change network in storage:
        window.localStorage.setItem(LOCALSTORE_RPC_KEY, network);
    }, [network]);

    return [network, setNetwork];
}
