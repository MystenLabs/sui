// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	IdentifierRecord,
	StandardConnectFeature,
	StandardDisconnectFeature,
	StandardEventsFeature,
	WalletWithFeatures,
} from '@wallet-standard/core';

import type { SuiSignAndExecuteTransactionBlockFeature } from './suiSignAndExecuteTransactionBlock.js';
import type { SuiSignMessageFeature } from './suiSignMessage.js';
import type { SuiSignPersonalMessageFeature } from './suiSignPersonalMessage.js';
import type { SuiSignTransactionBlockFeature } from './suiSignTransactionBlock.js';

/**
 * Wallet Standard features that are unique to Sui, and that all Sui wallets are expected to implement.
 */
export type SuiFeatures = SuiSignTransactionBlockFeature &
	SuiSignAndExecuteTransactionBlockFeature &
	SuiSignPersonalMessageFeature &
	// This deprecated feature should be removed once wallets update to the new method:
	Partial<SuiSignMessageFeature>;

export type WalletWithSuiFeatures = WalletWithFeatures<
	StandardConnectFeature &
		StandardEventsFeature &
		SuiFeatures &
		// Disconnect is an optional feature:
		Partial<StandardDisconnectFeature>
>;

/**
 * Represents a wallet with the absolute minimum feature set required to function in the Sui ecosystem.
 */
export type WalletWithRequiredFeatures = WalletWithFeatures<
	MinimallyRequiredFeatures &
		Partial<SuiFeatures> &
		Partial<StandardDisconnectFeature> &
		IdentifierRecord<unknown>
>;

export type MinimallyRequiredFeatures = StandardConnectFeature & StandardEventsFeature;

export * from './suiSignMessage.js';
export * from './suiSignTransactionBlock.js';
export * from './suiSignAndExecuteTransactionBlock.js';
export * from './suiSignPersonalMessage.js';
