// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SentryHttpTransport } from '@mysten/core';
import { SuiClientGraphQLTransport } from '@mysten/graphql-transport';
import { SuiClient, SuiHTTPTransport, getFullnodeUrl } from '@mysten/sui.js/client';

export enum Network {
	LOCAL = 'LOCAL',
	DEVNET = 'DEVNET',
	TESTNET = 'TESTNET',
	MAINNET = 'MAINNET',
}

export const NetworkConfigs: Record<Network, { url: string; graphqlUrl?: string }> = {
	[Network.LOCAL]: { url: getFullnodeUrl('localnet') },
	[Network.DEVNET]: { url: 'https://explorer-rpc.devnet.sui.io:443' },
	[Network.TESTNET]: {
		url: 'https://sui-testnet.mystenlabs.com/json-rpc',
		graphqlUrl: 'https://sui-testnet.mystenlabs.com/graphql',
	},
	[Network.MAINNET]: {
		url: 'https://sui-mainnet.mystenlabs.com/json-rpc',
		graphqlUrl: 'https://sui-mainnet.mystenlabs.com/graphql',
	},
};

const defaultClientMap: Map<Network | string, SuiClient> = new Map();

// NOTE: This class should not be used directly in React components, prefer to use the useSuiClient() hook instead
export const createSuiClient = (network: Network | string) => {
	const existingClient = defaultClientMap.get(network);
	if (existingClient) return existingClient;

	const networkUrl = network in Network ? NetworkConfigs[network as Network].url : network;
	const networkGraphqlUrl =
		network in Network ? NetworkConfigs[network as Network].graphqlUrl : undefined;

	const searchParams = new URLSearchParams(window.location.search);
	const suiClientTransportMode =
		networkGraphqlUrl && searchParams.get('forceGraphQL') === 'true' ? 'graphql' : 'http';

	const client = new SuiClient({
		transport:
			network in Network && network === Network.MAINNET
				? new SentryHttpTransport({
						url: networkUrl,
						graphqlUrl: networkGraphqlUrl,
						mode: suiClientTransportMode,
				  })
				: suiClientTransportMode === 'graphql'
				? new SuiClientGraphQLTransport({
						url: networkGraphqlUrl!,
						fallbackFullNodeUrl: networkUrl,
				  })
				: new SuiHTTPTransport({ url: networkUrl }),
	});
	defaultClientMap.set(network, client);
	return client;
};
