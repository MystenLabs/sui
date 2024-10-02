// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PublicKey } from '@mysten/sui/cryptography';
import type { ZkLoginSignatureInputs } from '@mysten/sui/zklogin';

import type { AuthProvider } from '../EnokiFlow.js';

export type EnokiNetwork = 'mainnet' | 'testnet' | 'devnet';
export type EnokiDomainNetwork = 'mainnet' | 'testnet';
export type EnokiSubanameStatus = 'PENDING' | 'ACTIVE';

export interface GetAppApiInput {}
export interface GetAppApiResponse {
	allowedOrigins: string[];
	authenticationProviders: {
		providerType: AuthProvider;
		clientId: string;
	}[];
	domains: {
		nftId: string;
		name: string;
		network: EnokiDomainNetwork;
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

export interface GetSubnamesApiInput {
	address?: string;
	network?: EnokiDomainNetwork;
	domain?: string;
}
export interface GetSubnamesApiResponse {
	subnames: {
		name: string;
		status: EnokiSubanameStatus;
	}[];
}

export type CreateSubnameApiInput = {
	domain: string;
	network?: EnokiDomainNetwork;
	subname: string;
} & (
	| {
			jwt: string;
			targetAddress?: never;
	  }
	| {
			targetAddress: string;
			jwt?: never;
	  }
);
export interface CreateSubnameApiResponse {
	name: string;
}

export interface DeleteSubnameApiInput {
	domain: string;
	network?: EnokiDomainNetwork;
	subname: string;
}
export interface DeleteSubnameApiResponse {
	name: string;
}
