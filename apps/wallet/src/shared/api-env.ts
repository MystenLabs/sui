// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export enum API_ENV {
	mainnet = 'mainnet',
	devNet = 'devNet',
	testNet = 'testNet',
	local = 'local',
	customRPC = 'customRPC',
}

export const networkNames: Record<API_ENV, string> = {
	[API_ENV.local]: 'Local',
	[API_ENV.testNet]: 'Testnet',
	[API_ENV.devNet]: 'Devnet',
	[API_ENV.mainnet]: 'Mainnet',
	[API_ENV.customRPC]: 'Custom RPC',
};

export type NetworkEnvType =
	| { env: Exclude<API_ENV, API_ENV.customRPC>; customRpcUrl: null }
	| { env: API_ENV.customRPC; customRpcUrl: string };

export const ENV_TO_API: Record<API_ENV, string | null> = {
	[API_ENV.customRPC]: null,
	[API_ENV.local]: process.env.API_ENDPOINT_LOCAL_FULLNODE || '',
	[API_ENV.devNet]: process.env.API_ENDPOINT_DEV_NET_FULLNODE || '',
	[API_ENV.testNet]: process.env.API_ENDPOINT_TEST_NET_FULLNODE || '',
	[API_ENV.mainnet]: process.env.API_ENDPOINT_MAINNET_FULLNODE || '',
};
