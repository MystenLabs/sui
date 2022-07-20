// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    createContext,
    useEffect,
    useState,
    type Dispatch,
    type SetStateAction,
} from 'react';

import { Network } from './utils/api/DefaultRpcClient';
import { IS_LOCAL_ENV, IS_STAGING_ENV, CURRENT_ENV } from './utils/envUtil';

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
    let defaultNetwork: string | null;

    // If running yarn start:local, ignore what is in storage and use Local network:
    if (IS_LOCAL_ENV) {
        defaultNetwork = Network.Local;
    } else {
        // Default network is that in storage, unless this is
        // null or was set a long time ago, then instead use website's default value:
        defaultNetwork = window.localStorage.getItem(LOCALSTORE_RPC_KEY);
        if (!defaultNetwork || wasNetworkSetLongTimeAgo()) {
            defaultNetwork = IS_STAGING_ENV ? Network.Staging : Network.Devnet;
            window.localStorage.setItem(LOCALSTORE_RPC_KEY, defaultNetwork);
        }
    }

    const [network, setNetwork] = useState<Network | string>(defaultNetwork);

    useEffect(() => {
        // If network in UI changes, change network in storage:
        window.localStorage.setItem(LOCALSTORE_RPC_KEY, network);
    }, [network]);

    return [network, setNetwork];
}
