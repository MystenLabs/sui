// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	ExecuteTransactionRequestType,
	SuiTransactionBlockResponse,
	SuiTransactionBlockResponseOptions,
} from '@mysten/sui.js/client';

import type { SuiSignTransactionBlockInput } from './suiSignTransactionBlock.js';

/** The latest API version of the signAndExecuteTransactionBlock API. */
export type SuiSignAndExecuteTransactionBlockVersion = '1.0.0';

/**
 * A Wallet Standard feature for signing a transaction, and submitting it to the
 * network. The wallet is expected to submit the transaction to the network via RPC,
 * and return the transaction response.
 */
export type SuiSignAndExecuteTransactionBlockFeature = {
	/** Namespace for the feature. */
	'sui:signAndExecuteTransactionBlock': {
		/** Version of the feature API. */
		version: SuiSignAndExecuteTransactionBlockVersion;
		signAndExecuteTransactionBlock: SuiSignAndExecuteTransactionBlockMethod;
	};
};

export type SuiSignAndExecuteTransactionBlockMethod = (
	input: SuiSignAndExecuteTransactionBlockInput,
) => Promise<SuiSignAndExecuteTransactionBlockOutput>;

/** Input for signing and sending transactions. */
export interface SuiSignAndExecuteTransactionBlockInput extends SuiSignTransactionBlockInput {
	/**
	 * `WaitForEffectsCert` or `WaitForLocalExecution`, see details in `ExecuteTransactionRequestType`.
	 * Defaults to `WaitForLocalExecution` if options.showEffects or options.showEvents is true
	 */
	requestType?: ExecuteTransactionRequestType;
	/** specify which fields to return (e.g., transaction, effects, events, etc). By default, only the transaction digest will be returned. */
	options?: SuiTransactionBlockResponseOptions;
}

/** Output of signing and sending transactions. */
export interface SuiSignAndExecuteTransactionBlockOutput extends SuiTransactionBlockResponse {}
