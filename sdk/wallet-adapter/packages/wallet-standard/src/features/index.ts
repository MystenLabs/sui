// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithFeatures } from "@wallet-standard/core";
import type { SuiSignAndExecuteTransactionFeature } from "./suiSignAndExecuteTransaction";

/**
 * Wallet Standard features that are unique to Sui, and that all Sui wallets are expected to implement.
 */
export type SuiFeatures = SuiSignAndExecuteTransactionFeature;

export type WalletWithSuiFeatures = WalletWithFeatures<SuiFeatures>;

export * from "./suiSignAndExecuteTransaction";
