// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createClient, createConfig  } from '@hey-api/client-fetch';
import type {Options} from '@hey-api/client-fetch';
import type { PostV1ZkloginNonceData, PostV1ZkloginNonceError, PostV1ZkloginNonceResponse, PostV1ZkloginZkpData, PostV1ZkloginZkpError, PostV1ZkloginZkpResponse, PostV1TransactionBlocksSponsorData, PostV1TransactionBlocksSponsorError, PostV1TransactionBlocksSponsorResponse, PostV1TransactionBlocksSponsorByDigestData, PostV1TransactionBlocksSponsorByDigestError, PostV1TransactionBlocksSponsorByDigestResponse, GetV1ZkloginData, GetV1ZkloginError, GetV1ZkloginResponse, GetV1AppError, GetV1AppResponse } from './types.gen.js';

export const client = createClient(createConfig());

/**
 * Create zkLogin nonce
 * Generates a nonce used in the zkLogin OAuth flow. Using this API is not required, you can also construct a nonce client-side if desired.
 */
export const postV1ZkloginNonce = <ThrowOnError extends boolean = false>(options?: Options<PostV1ZkloginNonceData, ThrowOnError>) => { return (options?.client ?? client).post<ThrowOnError, PostV1ZkloginNonceResponse, PostV1ZkloginNonceError>({
    ...options,
    url: '/v1/zklogin/nonce'
}); };

/**
 * Create zkLogin ZKP
 * Creates a zero-knowledge proof, which is used to submit transactions to Sui.
 */
export const postV1ZkloginZkp = <ThrowOnError extends boolean = false>(options: Options<PostV1ZkloginZkpData, ThrowOnError>) => { return (options?.client ?? client).post<ThrowOnError, PostV1ZkloginZkpResponse, PostV1ZkloginZkpError>({
    ...options,
    url: '/v1/zklogin/zkp'
}); };

/**
 * Create sponsored transaction
 * Creates sponsored transaction
 */
export const postV1TransactionBlocksSponsor = <ThrowOnError extends boolean = false>(options?: Options<PostV1TransactionBlocksSponsorData, ThrowOnError>) => { return (options?.client ?? client).post<ThrowOnError, PostV1TransactionBlocksSponsorResponse, PostV1TransactionBlocksSponsorError>({
    ...options,
    url: '/v1/transaction-blocks/sponsor'
}); };

/**
 * Submits a sponsored transaction for execution
 * Submits a transaction created from `/transaction-blocks/sponsor` for execution.
 */
export const postV1TransactionBlocksSponsorByDigest = <ThrowOnError extends boolean = false>(options: Options<PostV1TransactionBlocksSponsorByDigestData, ThrowOnError>) => { return (options?.client ?? client).post<ThrowOnError, PostV1TransactionBlocksSponsorByDigestResponse, PostV1TransactionBlocksSponsorByDigestError>({
    ...options,
    url: '/v1/transaction-blocks/sponsor/{digest}'
}); };

/**
 * Get address for zkLogin user
 * Returns the address and salt value for the given JWT. If the JWT is not valid, the API will return an error code.
 */
export const getV1Zklogin = <ThrowOnError extends boolean = false>(options: Options<GetV1ZkloginData, ThrowOnError>) => { return (options?.client ?? client).get<ThrowOnError, GetV1ZkloginResponse, GetV1ZkloginError>({
    ...options,
    url: '/v1/zklogin'
}); };

/**
 * Get app metadata
 * Returns the public metadata (configured in the Enoki Developer Portal) of the app associated with the API key.
 */
export const getV1App = <ThrowOnError extends boolean = false>(options?: Options<unknown, ThrowOnError>) => { return (options?.client ?? client).get<ThrowOnError, GetV1AppResponse, GetV1AppError>({
    ...options,
    url: '/v1/app'
}); };