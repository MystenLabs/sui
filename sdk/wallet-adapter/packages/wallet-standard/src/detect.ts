// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ConnectFeature } from "@wallet-standard/features";
import { Wallet, WalletWithFeatures } from "@wallet-standard/standard";
import { SuiSignAndExecuteTransactionFeature } from "./features";

export type StandardWalletAdapterWallet = WalletWithFeatures<
  ConnectFeature & SuiSignAndExecuteTransactionFeature
>;

export function isStandardWalletAdapterCompatibleWallet(
  wallet: Wallet
): wallet is StandardWalletAdapterWallet {
  return (
    "standard:connect" in wallet.features &&
    "sui:signAndExecuteTransaction" in wallet.features
  );
}
