// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureIsOn } from '@growthbook/growthbook-react';
import { useGetSystemState } from '@mysten/core';
import { useContext } from 'react';

import { NetworkContext } from '../../context';
import { Network } from '../../utils/api/DefaultRpcClient';

import { NetworkSelect, type NetworkOption } from '~/ui/header/NetworkSelect';

export default function WrappedNetworkSelect() {
    const [network, setNetwork] = useContext(NetworkContext);
    const mainnetEnabled = useFeatureIsOn('mainnet-selection');
    const { data } = useGetSystemState();

    const networks = [
        ...(mainnetEnabled ? [{ id: Network.MAINNET, label: 'Mainnet' }] : []),
        { id: Network.TESTNET, label: 'Testnet' },
        { id: Network.DEVNET, label: 'Devnet' },
        { id: Network.LOCAL, label: 'Local' },
    ].filter(Boolean) as NetworkOption[];

    return (
        <NetworkSelect
            value={network}
            onChange={setNetwork}
            networks={networks}
            version={data?.protocolVersion}
        />
    );
}
