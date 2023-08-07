// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount } from '@wallet-standard/core';

/**
 * The latest API version of the signMessage API.
 * @deprecated Wallets can still implement this method for compatibility, but this has been replaced by the `sui:signPersonalMessage` feature
 */
export type SuiSignMessageVersion = '1.0.0';

/**
 * A Wallet Standard feature for signing a personal message, and returning the
 * message bytes that were signed, and message signature.
 *
 * @deprecated Wallets can still implement this method for compatibility, but this has been replaced by the `sui:signPersonalMessage` feature
 */
export type SuiSignMessageFeature = {
	/** Namespace for the feature. */
	'sui:signMessage': {
		/** Version of the feature API. */
		version: SuiSignMessageVersion;
		signMessage: SuiSignMessageMethod;
	};
};

/** @deprecated Wallets can still implement this method for compatibility, but this has been replaced by the `sui:signPersonalMessage` feature */
export type SuiSignMessageMethod = (input: SuiSignMessageInput) => Promise<SuiSignMessageOutput>;

/**
 * Input for signing messages.
 * @deprecated Wallets can still implement this method for compatibility, but this has been replaced by the `sui:signPersonalMessage` feature
 */
export interface SuiSignMessageInput {
	message: Uint8Array;
	account: WalletAccount;
}

/**
 * Output of signing messages.
 * @deprecated Wallets can still implement this method for compatibility, but this has been replaced by the `sui:signPersonalMessage` feature
 */
export interface SuiSignMessageOutput {
	messageBytes: string;
	signature: string;
}
