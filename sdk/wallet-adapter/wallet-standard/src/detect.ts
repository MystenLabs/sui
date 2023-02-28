// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  ConnectFeature,
  DisconnectFeature,
  EventsFeature,
  Wallet,
  WalletWithFeatures,
} from "@wallet-standard/core";
import { SuiFeatures } from "./features";

export type StandardWalletAdapterWallet = WalletWithFeatures<
  ConnectFeature &
    EventsFeature &
    SuiFeatures &
    // Disconnect is an optional feature:
    Partial<DisconnectFeature>
>;

// TODO: Enable filtering by subset of features:
export function isStandardWalletAdapterCompatibleWallet(
  wallet: Wallet
): wallet is StandardWalletAdapterWallet {
  return (
    "standard:connect" in wallet.features &&
    "standard:events" in wallet.features &&
    // TODO: Enable once ecosystem wallets adopt this:
    // "sui:signTransaction" in wallet.features &&
    // TODO: Enable once ecosystem wallets adopt this
    // "sui:signMessage" in wallet.features &&
    "sui:signAndExecuteTransaction" in wallet.features
  );
}
