// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { WalletAdapterList } from "@mysten/wallet-adapter-base";
import { WalletProvider } from "@mysten/wallet-adapter-react";
import { WalletStandardAdapterProvider } from "@mysten/wallet-adapter-wallet-standard";
import { ReactNode, useMemo } from "react";

export * from "./ConnectButton";
export * from './ConnectModal';
export * from "@mysten/wallet-adapter-react";

interface WalletKitProviderProps {
  adapters?: WalletAdapterList;
  children: ReactNode;
}

export function WalletKitProvider({
  adapters: configuredAdapters,
  children,
}: WalletKitProviderProps) {
  const adapters = useMemo(
    () => configuredAdapters ?? [new WalletStandardAdapterProvider()],
    [configuredAdapters]
  );

  return <WalletProvider adapters={adapters}>{children}</WalletProvider>;
}
