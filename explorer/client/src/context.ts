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
import { IS_LOCAL_ENV } from './utils/envUtil';

export const NetworkContext = createContext<
    [Network | string, Dispatch<SetStateAction<Network | string>>]
>(['', () => null]);

const LOCALSTORE_RPC_KEY = 'sui-explorer-rpc';

export function useNetwork(): [
    string,
    Dispatch<SetStateAction<Network | string>>
] {
    //Get storage network from browser:
    const storageNetwork = window.localStorage.getItem(LOCALSTORE_RPC_KEY);

    // Start value is that in storage or, if null, the website's default:
    const [network, setNetwork] = useState<Network | string>(
        storageNetwork || (IS_LOCAL_ENV ? Network.Local : Network.Devnet)
    );

    useEffect(() => {
        // If network in UI changes, change network in storage:
        window.localStorage.setItem(LOCALSTORE_RPC_KEY, network);
    }, [network]);

    return [network, setNetwork];
}
