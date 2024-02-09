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

export const NetworkConfigs: Record<Network, { url: string }> = {
	[Network.LOCAL]: { url: getFullnodeUrl('localnet') },
	[Network.DEVNET]: { url: 'https://explorer-rpc.devnet.sui.io:443' },
	[Network.TESTNET]: { url: 'https://sui-testnet.mystenlabs.com/json-rpc' },
	[Network.MAINNET]: { url: 'https://sui-mainnet.mystenlabs.com/json-rpc' },
};

const defaultClientMap: Map<Network | string, SuiClient> = new Map();

// NOTE: This class should not be used directly in React components, prefer to use the useSuiClient() hook instead
export const createSuiClient = (network: Network | string) => {
	const existingClient = defaultClientMap.get(network);
	if (existingClient) return existingClient;

	const networkUrl = network in Network ? NetworkConfigs[network as Network].url : network;

	const client = new SuiClient({
		transport:
			network in Network && network === Network.MAINNET
				? new SentryHttpTransport(networkUrl)
				: new SuiHTTPTransport({ url: networkUrl }),
	});
	defaultClientMap.set(network, client);
	return client;
};
