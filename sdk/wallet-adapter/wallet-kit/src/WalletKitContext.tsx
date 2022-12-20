// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  createContext,
  ReactNode,
  useContext,
  useMemo,
  useRef,
  useSyncExternalStore,
} from "react";
import { WalletKitCore, WalletKitCoreState } from "@mysten/wallet-kit-core";
import { WalletStandardAdapterProvider } from "@mysten/wallet-adapter-wallet-standard";
import { UnsafeBurnerWalletAdapter } from "@mysten/wallet-adapter-unsafe-burner";
import { WalletAdapterList } from "@mysten/wallet-adapter-base";

export const WalletKitContext = createContext<WalletKitCore | null>(null);

interface WalletKitProviderProps {
  adapters?: WalletAdapterList;
  /** Enable the development-only unsafe burner wallet, which is can be useful for testing. */
  enableUnsafeBurner?: boolean;
  children: ReactNode;
}

export function WalletKitProvider({
  adapters: configuredAdapters,
  children,
  enableUnsafeBurner,
}: WalletKitProviderProps) {
  const adapters = useMemo(
    () =>
      configuredAdapters ?? [
        new WalletStandardAdapterProvider(),
        ...(enableUnsafeBurner ? [new UnsafeBurnerWalletAdapter()] : []),
      ],
    [configuredAdapters]
  );

  const walletKitRef = useRef<WalletKitCore | null>(null);
  if (!walletKitRef.current) {
    walletKitRef.current = new WalletKitCore({ adapters });
  }

  return (
    <WalletKitContext.Provider value={walletKitRef.current}>
      {children}
    </WalletKitContext.Provider>
  );
}

type UseWalletKit = WalletKitCoreState &
  Pick<WalletKitCore, "connect" | "disconnect" | "signAndExecuteTransaction">;

export function useWalletKit(): UseWalletKit {
  const walletKit = useContext(WalletKitContext);

  if (!walletKit) {
    throw new Error(
      "You must call `useWalletKit` within the of the `WalletKitProvider`."
    );
  }

  const state = useSyncExternalStore(walletKit.subscribe, walletKit.getState);

  return useMemo(
    () => ({
      connect: walletKit.connect,
      disconnect: walletKit.disconnect,
      signAndExecuteTransaction: walletKit.signAndExecuteTransaction,
      ...state,
    }),
    [walletKit, state]
  );
}
