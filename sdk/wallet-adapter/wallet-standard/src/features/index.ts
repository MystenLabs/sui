// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithFeatures } from '@wallet-standard/core';
import type { SuiSignTransactionBlockFeature } from './suiSignTransactionBlock';
import type { SuiSignAndExecuteTransactionBlockFeature } from './suiSignAndExecuteTransactionBlock';
import { SuiSignMessageFeature } from './suiSignMessage';
import { SuiSignPersonalMessageFeature } from './suiSignPersonalMessage';

/**
 * Wallet Standard features that are unique to Sui, and that all Sui wallets are expected to implement.
 */
export type SuiFeatures = SuiSignTransactionBlockFeature &
	SuiSignAndExecuteTransactionBlockFeature &
	SuiSignPersonalMessageFeature &
	// This deprecated feature should be removed once wallets update to the new method:
	Partial<SuiSignMessageFeature>;

export type WalletWithSuiFeatures = WalletWithFeatures<SuiFeatures>;

export * from './suiSignMessage';
export * from './suiSignTransactionBlock';
export * from './suiSignAndExecuteTransactionBlock';
export * from './suiSignPersonalMessage';
