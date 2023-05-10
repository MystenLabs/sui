// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// eslint-disable-next-line no-restricted-imports
import { useSearchParams } from 'react-router-dom';

import { Network } from '~/utils/api/DefaultRpcClient';

export const DEFAULT_NETWORK =
    (import.meta.env.VITE_NETWORK as string) ||
    (import.meta.env.DEV ? Network.LOCAL : Network.MAINNET);

export function useNetwork() {
    const [searchParams, setSearchParams] = useSearchParams();
    const networkParam = searchParams.get('network');
    const network = networkParam ? getNetwork(networkParam) : DEFAULT_NETWORK;

    const setNetwork = (network: Network | string) => {
        setSearchParams({ network: network.toLowerCase() });
    };

    return [network, setNetwork] as const;
}

function getNetwork(rawNetwork: string) {
    const uppercasedRawNetwork = rawNetwork.toUpperCase();
    const networks = Object.values<string>(Network);
    return networks.includes(uppercasedRawNetwork)
        ? uppercasedRawNetwork
        : rawNetwork;
}
