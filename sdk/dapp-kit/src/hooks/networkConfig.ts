// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClientOptions } from '@mysten/sui.js/client';

import { useSuiClientContext } from './useSuiClient.js';

export type NetworkConfig<T extends object = object> = SuiClientOptions & {
	variables?: T;
};

export function createNetworkConfig<
	const T extends Record<string, Config>,
	Config extends NetworkConfig<Variables> = T[keyof T],
	Variables extends object = NonNullable<Config['variables']>,
>(networkConfig: T) {
	function useNetworkConfig(): Config {
		const { config } = useSuiClientContext();

		if (!config) {
			throw new Error('No network config found');
		}

		return config as T[keyof T];
	}

	function useNetworkVariables(): Variables {
		const { variables } = useNetworkConfig();

		return (variables ?? {}) as Variables;
	}

	function useNetworkVariable<K extends keyof Variables>(name: K): Variables[K] {
		const variables = useNetworkVariables();

		return variables[name];
	}

	return {
		networkConfig,
		useNetworkConfig,
		useNetworkVariables,
		useNetworkVariable,
	};
}
