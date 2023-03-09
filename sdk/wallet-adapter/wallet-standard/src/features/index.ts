// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithFeatures } from "@wallet-standard/core";
import type { SuiSignTransactionFeature } from "./suiSignTransaction";
import type { SuiSignAndExecuteTransactionFeature } from "./suiSignAndExecuteTransaction";
import { SuiSignMessageFeature } from "./suiSignMessage";

/**
 * Wallet Standard features that are unique to Sui, and that all Sui wallets are expected to implement.
 */
export type SuiFeatures = SuiSignTransactionFeature &
  SuiSignAndExecuteTransactionFeature &
  SuiSignMessageFeature;

export type WalletWithSuiFeatures = WalletWithFeatures<SuiFeatures>;

export * from "./suiSignMessage";
export * from "./suiSignTransaction";
export * from "./suiSignAndExecuteTransaction";
