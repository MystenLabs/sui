// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithFeatures } from "@wallet-standard/standard";
import type { SuiSignAndExecuteTransactionFeature } from "./suiSignAndExecuteTransaction";
import { SuiSignMessageFeature } from './suiSignMessage';

/**
 * Wallet Standard features that are unique to Sui, and that all Sui wallets are expected to implement.
 */
export type SuiFeatures = SuiSignMessageFeature & SuiSignAndExecuteTransactionFeature;

export type WalletWithSuiFeatures = WalletWithFeatures<SuiFeatures>;

export * from "./suiSignAndExecuteTransaction";
export * from "./suiSignMessage";
