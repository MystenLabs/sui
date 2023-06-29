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
