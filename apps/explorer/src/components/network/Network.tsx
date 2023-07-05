// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetSystemState, useGetBinaryVersion } from '@mysten/core';
import { useContext } from 'react';

import { NetworkContext } from '../../context';
import { Network } from '../../utils/api/DefaultRpcClient';
import { NetworkSelect, type NetworkOption } from '~/ui/header/NetworkSelect';
import { ampli } from '~/utils/analytics/ampli';

export default function WrappedNetworkSelect() {
	const [network, setNetwork] = useContext(NetworkContext);
	const { data } = useGetSystemState();
	const { data: binaryVersion } = useGetBinaryVersion();

	const networks = [
		{ id: Network.MAINNET, label: 'Mainnet' },
		{ id: Network.TESTNET, label: 'Testnet' },
		{ id: Network.DEVNET, label: 'Devnet' },
		{ id: Network.LOCAL, label: 'Local' },
	].filter(Boolean) as NetworkOption[];

	return (
		<NetworkSelect
			value={network}
			onChange={(networkId) => {
				ampli.switchedNetwork({ toNetwork: networkId });
				setNetwork(networkId);
			}}
			networks={networks}
			version={data?.protocolVersion}
			binaryVersion={binaryVersion}
		/>
	);
}
