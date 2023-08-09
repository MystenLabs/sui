// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';
import type { SuiClientOptions } from '@mysten/sui.js/client';
import { createContext, useMemo, useState } from 'react';

type NetworkConfig = SuiClient | SuiClientOptions;
type NetworkConfigs<T extends NetworkConfig = NetworkConfig> = Record<string, T>;

export interface SuiClientProviderContext {
	client: SuiClient;
	queryKey: (key: unknown[]) => unknown[];
	networks: NetworkConfigs;
	selectNetwork: (network: string) => void;
}

export const SuiClientContext = createContext<SuiClientProviderContext | undefined>(undefined);

export interface SuiClientProviderProps<T extends NetworkConfigs> {
	networks?: T;
	createClient?: (name: keyof T, config: T[keyof T]) => SuiClient;
	defaultNetwork?: keyof T;
	children: React.ReactNode;
}

export function SuiClientProvider<T extends NetworkConfigs>(props: SuiClientProviderProps<T>) {
	const networks = useMemo(
		() =>
			props.networks ??
			({
				devnet: { url: getFullnodeUrl('devnet') },
			} as unknown as T),
		[props.networks],
	);
	const [selectedNetwork, setSelectedNetwork] = useState<keyof T>(
		props.defaultNetwork ?? Object.keys(networks)[0],
	);

	const createClient = useMemo(() => {
		if (props.createClient) {
			return props.createClient;
		}

		return (_name: keyof T, config: T[keyof T]) => {
			if (config instanceof SuiClient) {
				return config;
			}

			return new SuiClient(config);
		};
	}, [props.createClient]);

	const [client, setClient] = useState<SuiClient>(() => {
		return createClient(selectedNetwork, networks[selectedNetwork]);
	});

	const ctx = useMemo((): SuiClientProviderContext => {
		return {
			client,
			queryKey: (key: unknown[]) => [selectedNetwork, ...key],
			networks,
			selectNetwork: (network: keyof T) => {
				if (network !== selectedNetwork) {
					setSelectedNetwork(network);
					setClient(createClient(network, networks[network]));
				}
			},
		};
	}, [client, setClient, createClient, selectedNetwork, networks]);

	return <SuiClientContext.Provider value={ctx}>{props.children}</SuiClientContext.Provider>;
}
