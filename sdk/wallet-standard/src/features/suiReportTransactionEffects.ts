// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { IdentifierString, WalletAccount } from '@wallet-standard/core';

/**
 * A Wallet Standard feature for reporting the effects of a transaction block executed by a dapp
 * The feature allows wallets to updated their caches using the effects of the transaction
 * executed outside of the wallet
 */
export type SuiReportTransactionEffectsFeature = {
	/** Namespace for the feature. */
	'sui:reportTransactionEffects': {
		/** Version of the feature API. */
		version: '1.0.0';
		reportTransactionEffects: SuiReportTransactionEffectsMethod;
	};
};

export type SuiReportTransactionEffectsMethod = (
	input: SuiReportTransactionEffectsInput,
) => Promise<void>;

/** Input for signing transactions. */
export interface SuiReportTransactionEffectsInput {
	account: WalletAccount;
	chain: IdentifierString;
	/** Transaction effects as base64 encoded bcs. */
	effects: string;
}
