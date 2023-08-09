// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SentryHttpTransport } from '@mysten/core';
import { SuiClient, SuiHTTPTransport, getFullnodeUrl } from '@mysten/sui.js/client';

export enum Network {
	LOCAL = 'LOCAL',
	DEVNET = 'DEVNET',
	TESTNET = 'TESTNET',
	MAINNET = 'MAINNET',
}

const CONNECTIONS: Record<Network, string> = {
	[Network.LOCAL]: getFullnodeUrl('localnet'),
	[Network.DEVNET]: 'https://explorer-rpc.devnet.sui.io:443',
	[Network.TESTNET]: 'https://explorer-rpc.testnet.sui.io:443',
	[Network.MAINNET]: 'https://explorer-rpc.mainnet.sui.io:443',
};

const defaultRpcMap: Map<Network | string, SuiClient> = new Map();

// NOTE: This class should not be used directly in React components, prefer to use the useRpcClient() hook instead
export const DefaultRpcClient = (network: Network | string) => {
	const existingClient = defaultRpcMap.get(network);
	if (existingClient) return existingClient;

	const networkUrl = network in Network ? CONNECTIONS[network as Network] : network;

	const provider = new SuiClient({
		transport:
			network in Network && network === Network.MAINNET
				? new SentryHttpTransport(networkUrl)
				: new SuiHTTPTransport({ url: networkUrl }),
	});
	defaultRpcMap.set(network, provider);
	return provider;
};
