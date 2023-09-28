// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClientQuery } from '@mysten/dapp-kit';
import { useContext } from 'react';

import { NetworkContext } from '../../context';
import { Network } from '../../utils/api/DefaultRpcClient';
import { NetworkSelect, type NetworkOption } from '~/ui/header/NetworkSelect';
import { ampli } from '~/utils/analytics/ampli';

export default function WrappedNetworkSelect() {
	const [network, setNetwork] = useContext(NetworkContext);
	const { data } = useSuiClientQuery('getLatestSuiSystemState');
	const { data: binaryVersion } = useSuiClientQuery('getRpcApiVersion');

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
