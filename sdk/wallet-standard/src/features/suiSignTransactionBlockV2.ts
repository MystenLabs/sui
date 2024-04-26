// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { IdentifierString, WalletAccount } from '@wallet-standard/core';

/** The latest API version of the signTransactionBlock API. */
export type SuiSignTransactionBlockV2Version = '2.0.0';

/**
 * A Wallet Standard feature for signing a transaction, and returning the
 * serialized transaction and transaction signature.
 */
export type SuiSignTransactionBlockV2Feature = {
	/** Namespace for the feature. */
	'sui:signTransactionBlock:v2': {
		/** Version of the feature API. */
		version: SuiSignTransactionBlockV2Version;
		signTransactionBlock: SuiSignTransactionBlockV2Method;
	};
};

export type SuiSignTransactionBlockV2Method = (
	input: SuiSignTransactionBlockV2Input,
) => Promise<SuiSignTransactionBlockV2Output>;

/** Input for signing transactions. */
export interface SuiSignTransactionBlockV2Input {
	transactionBlock: string;
	account: WalletAccount;
	chain: IdentifierString;
}

/** Output of signing transactions. */
export interface SuiSignTransactionBlockV2Output {
	bytes: string;
	signature: string;
}
