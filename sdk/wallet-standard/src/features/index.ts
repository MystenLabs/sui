// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	IdentifierRecord,
	StandardConnectFeature,
	StandardDisconnectFeature,
	StandardEventsFeature,
	WalletWithFeatures,
} from '@wallet-standard/core';

import type { SuiReportTransactionEffectsFeature } from './suiReportTransactionEffects.js';
import type { SuiSignAndExecuteTransactionFeature } from './suiSignAndExecuteTransaction.js';
import type { SuiSignAndExecuteTransactionBlockFeature } from './suiSignAndExecuteTransactionBlock.js';
import type { SuiSignMessageFeature } from './suiSignMessage.js';
import type { SuiSignPersonalMessageFeature } from './suiSignPersonalMessage.js';
import type { SuiSignTransactionFeature } from './suiSignTransaction.js';
import type { SuiSignTransactionBlockFeature } from './suiSignTransactionBlock.js';

/**
 * Wallet Standard features that are unique to Sui, and that all Sui wallets are expected to implement.
 */
export type SuiFeatures = Partial<SuiSignTransactionBlockFeature> &
	Partial<SuiSignAndExecuteTransactionBlockFeature> &
	SuiSignPersonalMessageFeature &
	SuiSignAndExecuteTransactionFeature &
	SuiSignTransactionFeature &
	// This deprecated feature should be removed once wallets update to the new method:
	Partial<SuiSignMessageFeature> &
	Partial<SuiReportTransactionEffectsFeature>;

export type SuiWalletFeatures = StandardConnectFeature &
	StandardEventsFeature &
	SuiFeatures &
	// Disconnect is an optional feature:
	Partial<StandardDisconnectFeature>;

export type WalletWithSuiFeatures = WalletWithFeatures<SuiWalletFeatures>;

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
export * from './suiSignTransaction.js';
export * from './suiSignAndExecuteTransactionBlock.js';
export * from './suiSignAndExecuteTransaction.js';
export * from './suiSignPersonalMessage.js';
export * from './suiReportTransactionEffects.js';
