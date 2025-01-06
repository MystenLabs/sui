// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { API_ENV } from '_src/shared/api-env';

export enum RequestType {
	CONNECT = 'connect',
	SIGN_TRANSACTION = 'sign-transaction',
	SIGN_MESSAGE = 'sign-personal-message',
}

export type DappPreflightResponse = {
	block: {
		enabled: boolean;
		title: string;
		subtitle: string;
	};
	warnings?: {
		title: string;
		subtitle: string;
	}[];
};

export type Network = 'mainnet' | 'testnet' | 'devnet' | 'local';

export const API_ENV_TO_NETWORK: Record<API_ENV, Network> = {
	[API_ENV.local]: 'local',
	[API_ENV.devNet]: 'devnet',
	[API_ENV.testNet]: 'testnet',
	[API_ENV.mainnet]: 'mainnet',
	[API_ENV.customRPC]: 'mainnet', // treat custom RPC as mainnet for now
};

export type DappPreflightRequest = {
	network?: Network;
	requestType: RequestType;
	origin: string;
	transactionBytes?: string;
	message?: string;
};
