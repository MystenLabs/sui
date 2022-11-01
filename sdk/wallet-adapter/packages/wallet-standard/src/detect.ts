// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  ConnectFeature,
  DisconnectFeature,
  EventsFeature,
  Wallet,
  WalletWithFeatures,
} from "@wallet-standard/core";
import { SuiSignAndExecuteTransactionFeature } from "./features";

export type StandardWalletAdapterWallet = WalletWithFeatures<
  ConnectFeature &
    EventsFeature &
    SuiSignAndExecuteTransactionFeature &
    // Disconnect is an optional feature:
    Partial<DisconnectFeature>
>;

export function isStandardWalletAdapterCompatibleWallet(
  wallet: Wallet
): wallet is StandardWalletAdapterWallet {
  return (
    "standard:connect" in wallet.features &&
    "standard:events" in wallet.features &&
    "sui:signAndExecuteTransaction" in wallet.features
  );
}
