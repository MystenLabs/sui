// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * A Wallet Standard feature for reporting the effects of a transaction block executed by a dapp
 * The feature allows wallets to updated their caches using the effects of the transaction
 * executed outside of the wallet
 */
export type SuiReportTransactionBlockEffectsFeature = {
	/** Namespace for the feature. */
	'sui:reportTransactionBlockEffects': {
		/** Version of the feature API. */
		version: '1.0.0';
		reportTransactionBlockEffects: SuiReportTransactionBlockEffectsMethod;
	};
};

export type SuiReportTransactionBlockEffectsMethod = (
	input: SuiReportTransactionBlockEffectsInput,
) => Promise<void>;

/** Input for signing transactions. */
export interface SuiReportTransactionBlockEffectsInput {
	effects: string;
}
