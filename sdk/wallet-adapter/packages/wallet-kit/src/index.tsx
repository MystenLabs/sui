import { WalletAdapterList } from "@mysten/wallet-adapter-base";
import { WalletProvider } from "@mysten/wallet-adapter-react";
import { WalletStandardAdapterProvider } from "@mysten/wallet-adapter-wallet-standard";
import { ReactNode, useMemo } from "react";

export * from "./ConnectButton";

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
