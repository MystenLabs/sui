// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	SuiSignTransactionBlockV2Input,
	SuiSignTransactionBlockV2Output,
} from './suiSignTransactionBlockV2.js';

/** The latest API version of the signAndExecuteTransactionBlock API. */
export type SuiSignAndExecuteTransactionBlockV2Version = '2.0.0';

/**
 * A Wallet Standard feature for signing a transaction, and submitting it to the
 * network. The wallet is expected to submit the transaction to the network via RPC,
 * and return the transaction response.
 */
export type SuiSignAndExecuteTransactionBlockV2Feature = {
	/** Namespace for the feature. */
	'sui:signAndExecuteTransactionBlock:v2': {
		/** Version of the feature API. */
		version: SuiSignAndExecuteTransactionBlockV2Version;
		signAndExecuteTransactionBlock: SuiSignAndExecuteTransactionBlockV2Method;
	};
};

export type SuiSignAndExecuteTransactionBlockV2Method = (
	input: SuiSignAndExecuteTransactionBlockV2Input,
) => Promise<SuiSignAndExecuteTransactionBlockV2Output>;

/** Input for signing and sending transactions. */
export interface SuiSignAndExecuteTransactionBlockV2Input extends SuiSignTransactionBlockV2Input {}

/** Output of signing and sending transactions. */
export interface SuiSignAndExecuteTransactionBlockV2Output extends SuiSignTransactionBlockV2Output {
	digest: string;
	effects: string;
	balanceChanges:
		| {
				address: string;
				amount: string;
				coinType: string;
		  }[]
		| null;
	signal?: AbortSignal;
}
