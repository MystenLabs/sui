// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useContext, useEffect } from 'react';

import { NetworkContext } from '../../context';
import { Network } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { GROWTHBOOK_FEATURES } from '../../utils/growthbook';
import { plausible } from '../../utils/plausible';

import { NetworkSelect, type NetworkOption } from '~/ui/header/NetworkSelect';

export default function WrappedNetworkSelect() {
    const [network, setNetwork] = useContext(NetworkContext);

    const showTestNet = useFeature(
        GROWTHBOOK_FEATURES.USE_TEST_NET_ENDPOINT
    ).on;

    useEffect(() => {
        plausible.trackEvent('Network', {
            props: { name: network },
        });
    }, [network]);

    const networks = [
        { id: Network.DEVNET, label: 'Devnet' },
        showTestNet && { id: Network.TESTNET, label: 'Testnet' },
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
