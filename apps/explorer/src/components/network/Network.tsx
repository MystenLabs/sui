// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useGetSystemState } from '@mysten/core';
import { useContext } from 'react';

import { NetworkContext } from '../../context';
import { Network } from '../../utils/api/DefaultRpcClient';
import { GROWTHBOOK_FEATURES } from '../../utils/growthbook';

import { NetworkSelect, type NetworkOption } from '~/ui/header/NetworkSelect';

export default function WrappedNetworkSelect() {
    const [network, setNetwork] = useContext(NetworkContext);
    const { data } = useGetSystemState();
    const showTestNet = useFeature(
        GROWTHBOOK_FEATURES.USE_TEST_NET_ENDPOINT
    ).on;

    const networks = [
        { id: Network.DEVNET, label: 'Devnet' },
        showTestNet && { id: Network.TESTNET, label: 'Testnet' },
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
