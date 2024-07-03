// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PublicKey } from '@mysten/sui/cryptography';
import type { ZkLoginSignatureInputs } from '@mysten/sui/zklogin';

import type { AuthProvider } from '../EnokiFlow.js';

export type EnokiNetwork = 'mainnet' | 'testnet' | 'devnet';

export interface GetAppApiInput {}
export interface GetAppApiResponse {
	authenticationProviders: {
		providerType: AuthProvider;
		clientId: string;
	}[];
}

export interface GetZkLoginApiInput {
	jwt: string;
}
export interface GetZkLoginApiResponse {
	address: string;
	salt: string;
}

export interface CreateZkLoginNonceApiInput {
	network?: EnokiNetwork;
	ephemeralPublicKey: PublicKey;
	additionalEpochs?: number;
}
export interface CreateZkLoginNonceApiResponse {
	nonce: string;
	randomness: string;
	epoch: number;
	maxEpoch: number;
	estimatedExpiration: number;
}

export interface CreateZkLoginZkpApiInput {
	network?: EnokiNetwork;
	jwt: string;
	ephemeralPublicKey: PublicKey;
	randomness: string;
	maxEpoch: number;
}
export interface CreateZkLoginZkpApiResponse extends ZkLoginSignatureInputs {}

export type CreateSponsoredTransactionApiInput = {
	network?: EnokiNetwork;
	transactionKindBytes: string;
} & (
	| {
			jwt: string;
			sender?: never;
			allowedAddresses?: never;
			allowedMoveCallTargets?: never;
	  }
	| {
			sender: string;
			allowedAddresses?: string[];
			allowedMoveCallTargets?: string[];
			jwt?: never;
	  }
);

export interface CreateSponsoredTransactionApiResponse {
	bytes: string;
	digest: string;
}

export interface ExecuteSponsoredTransactionApiInput {
	digest: string;
	signature: string;
}

export interface ExecuteSponsoredTransactionApiResponse {
	digest: string;
}
