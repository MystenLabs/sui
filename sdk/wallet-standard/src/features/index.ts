// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	IdentifierRecord,
	StandardConnectFeature,
	StandardDisconnectFeature,
	StandardEventsFeature,
	WalletWithFeatures,
} from '@wallet-standard/core';

import type { SuiSignAndExecuteTransactionBlockFeature } from './suiSignAndExecuteTransactionBlock';
import { SuiSignMessageFeature } from './suiSignMessage';
import { SuiSignPersonalMessageFeature } from './suiSignPersonalMessage';
import type { SuiSignTransactionBlockFeature } from './suiSignTransactionBlock';

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

export * from './suiSignMessage';
export * from './suiSignTransactionBlock';
export * from './suiSignAndExecuteTransactionBlock';
export * from './suiSignPersonalMessage';
