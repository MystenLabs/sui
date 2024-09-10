// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import networkEnv from '_src/background/NetworkEnv';
import { API_ENV, ENV_TO_API, type NetworkEnvType } from '_src/shared/api-env';
import { SentryHttpTransport } from '@mysten/core';
import { SuiClient, SuiHTTPTransport } from '@mysten/sui/client';

const suiClientPerNetwork = new Map<string, SuiClient>();
const SENTRY_MONITORED_ENVS = [API_ENV.mainnet];

export function getSuiClient({ env, customRpcUrl }: NetworkEnvType): SuiClient {
	const key = `${env}_${customRpcUrl}`;
	if (!suiClientPerNetwork.has(key)) {
		const connection = customRpcUrl ? customRpcUrl : ENV_TO_API[env];
		if (!connection) {
			throw new Error(`API url not found for network env ${env} ${customRpcUrl}`);
		}
		suiClientPerNetwork.set(
			key,
			new SuiClient({
				transport:
					!customRpcUrl && SENTRY_MONITORED_ENVS.includes(env)
						? new SentryHttpTransport(connection)
						: new SuiHTTPTransport({ url: connection }),
			}),
		);
	}
	return suiClientPerNetwork.get(key)!;
}

export async function getActiveNetworkSuiClient(): Promise<SuiClient> {
	return getSuiClient(await networkEnv.getActiveNetwork());
}
