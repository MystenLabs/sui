import {
  WalletProvider,
  WalletProviderProps,
} from "@mysten/wallet-adapter-react";
import { WalletStandardAdapterProvider } from "@mysten/wallet-adapter-wallet-standard";
import { useMemo } from "react";

export * from "./ConnectButton";

interface WalletKitProviderProps extends Partial<WalletProviderProps> {}

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
