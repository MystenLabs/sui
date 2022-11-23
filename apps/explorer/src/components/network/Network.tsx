// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useContext } from 'react';

import { NetworkContext } from '../../context';
import { Network } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV, IS_STAGING_ENV } from '../../utils/envUtil';
import { GROWTHBOOK_FEATURES } from '../../utils/growthbook';

import { NetworkSelect, type NetworkOption } from '~/ui/header/NetworkSelect';

export default function WrappedNetworkSelect() {
    const [network, setNetwork] = useContext(NetworkContext);

    const showTestNet = useFeature(
        GROWTHBOOK_FEATURES.USE_TEST_NET_ENDPOINT
    ).on;

    const networks = [
        { id: Network.DEVNET, label: 'Devnet' },
        showTestNet && { id: Network.TESTNET, label: 'Testnet' },
        IS_STAGING_ENV && { id: Network.STAGING, label: 'Staging' },
        { id: Network.LOCAL, label: 'Local' },
        IS_STATIC_ENV && { id: Network.STATIC, label: 'Static' },
    ].filter(Boolean) as NetworkOption[];

    return (
        <NetworkSelect
            value={network}
            onChange={setNetwork}
            networks={networks}
        />
    );
}
