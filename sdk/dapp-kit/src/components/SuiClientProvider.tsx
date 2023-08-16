// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient, getFullnodeUrl, isSuiClient } from '@mysten/sui.js/client';
import type { SuiClientOptions } from '@mysten/sui.js/client';
import { createContext, useMemo, useState } from 'react';

type NetworkConfig = SuiClient | SuiClientOptions;
type NetworkConfigs<T extends NetworkConfig = NetworkConfig> = Record<string, T>;

export interface SuiClientProviderContext {
	client: SuiClient;
	networks: NetworkConfigs;
	selectedNetwork: string;
	selectNetwork: (network: string) => void;
}

export const SuiClientContext = createContext<SuiClientProviderContext | null>(null);

export interface SuiClientProviderProps<T extends NetworkConfigs> {
	networks?: T;
	createClient?: (name: keyof T, config: T[keyof T]) => SuiClient;
	defaultNetwork?: keyof T & string;
	children: React.ReactNode;
}

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
	const networks = (props.networks ?? DEFAULT_NETWORKS) as T;
	const createClient =
		(props.createClient as typeof DEFAULT_CREATE_CLIENT) ?? DEFAULT_CREATE_CLIENT;

	const [selectedNetwork, setSelectedNetwork] = useState<keyof T & string>(
		props.defaultNetwork ?? (Object.keys(networks)[0] as keyof T & string),
	);

	const [client, setClient] = useState<SuiClient>(() => {
		return createClient(selectedNetwork, networks[selectedNetwork]);
	});

	const ctx = useMemo((): SuiClientProviderContext => {
		return {
			client,
			networks,
			selectedNetwork,
			selectNetwork: (network) => {
				if (network !== selectedNetwork) {
					setSelectedNetwork(network);
					setClient(createClient(network, networks[network]));
				}
			},
		};
	}, [client, setClient, createClient, selectedNetwork, networks]);

	return <SuiClientContext.Provider value={ctx}>{props.children}</SuiClientContext.Provider>;
}
