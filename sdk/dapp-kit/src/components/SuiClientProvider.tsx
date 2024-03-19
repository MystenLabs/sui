// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, isSuiClient, SuiClient } from '@mysten/sui.js/client';
import type { SuiClientOptions } from '@mysten/sui.js/client';
import { createContext, useMemo, useState } from 'react';

import type { NetworkConfig } from '../hooks/networkConfig.js';

type NetworkConfigs<T extends NetworkConfig | SuiClient = NetworkConfig | SuiClient> = Record<
	string,
	T
>;

export interface SuiClientProviderContext {
	client: SuiClient;
	networks: NetworkConfigs;
	network: string;
	config: NetworkConfig | null;
	selectNetwork: (network: string) => void;
}

export const SuiClientContext = createContext<SuiClientProviderContext | null>(null);

export type SuiClientProviderProps<T extends NetworkConfigs> = {
	createClient?: (name: keyof T, config: T[keyof T]) => SuiClient;
	children: React.ReactNode;
	networks?: T;
	onNetworkChange?: (network: keyof T & string) => void;
} & (
	| {
			defaultNetwork?: keyof T & string;
			network?: never;
	  }
	| {
			defaultNetwork?: never;
			network?: keyof T & string;
	  }
);

const DEFAULT_NETWORKS = {
	localnet: { url: getFullnodeUrl('localnet') },
};

const DEFAULT_CREATE_CLIENT = function createClient(
	_name: string,
	config: NetworkConfig | SuiClient,
) {
	if (isSuiClient(config)) {
		return config;
	}

	return new SuiClient(config);
};

export function SuiClientProvider<T extends NetworkConfigs>(props: SuiClientProviderProps<T>) {
	const { onNetworkChange, network, children } = props;
	const networks = (props.networks ?? DEFAULT_NETWORKS) as T;
	const createClient =
		(props.createClient as typeof DEFAULT_CREATE_CLIENT) ?? DEFAULT_CREATE_CLIENT;

	const [selectedNetwork, setSelectedNetwork] = useState<keyof T & string>(
		props.network ?? props.defaultNetwork ?? (Object.keys(networks)[0] as keyof T & string),
	);

	const currentNetwork = props.network ?? selectedNetwork;

	const client = useMemo(() => {
		return createClient(currentNetwork, networks[currentNetwork]);
	}, [createClient, currentNetwork, networks]);

	const ctx = useMemo((): SuiClientProviderContext => {
		return {
			client,
			networks,
			network: currentNetwork,
			config:
				networks[currentNetwork] instanceof SuiClient
					? null
					: (networks[currentNetwork] as SuiClientOptions),
			selectNetwork: (newNetwork) => {
				if (currentNetwork === newNetwork) {
					return;
				}

				if (!network && newNetwork !== selectedNetwork) {
					setSelectedNetwork(newNetwork);
				}

				onNetworkChange?.(newNetwork);
			},
		};
	}, [client, networks, selectedNetwork, currentNetwork, network, onNetworkChange]);

	return <SuiClientContext.Provider value={ctx}>{children}</SuiClientContext.Provider>;
}
