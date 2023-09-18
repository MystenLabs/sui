// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount } from '@wallet-standard/core';

/** The latest API version of the signPersonalMessage API. */
export type SuiSignPersonalMessageVersion = '1.0.0';

/**
 * A Wallet Standard feature for signing a personal message, and returning the
 * message bytes that were signed, and message signature.
 */
export type SuiSignPersonalMessageFeature = {
	/** Namespace for the feature. */
	'sui:signPersonalMessage': {
		/** Version of the feature API. */
		version: SuiSignPersonalMessageVersion;
		signPersonalMessage: SuiSignPersonalMessageMethod;
	};
};

export type SuiSignPersonalMessageMethod = (
	input: SuiSignPersonalMessageInput,
) => Promise<SuiSignPersonalMessageOutput>;

/** Input for signing personal messages. */
export interface SuiSignPersonalMessageInput {
	message: Uint8Array;
	account: WalletAccount;
}

/** Output of signing personal messages. */
export interface SuiSignPersonalMessageOutput extends SignedPersonalMessage {}

export interface SignedPersonalMessage {
	bytes: string;
	signature: string;
}
