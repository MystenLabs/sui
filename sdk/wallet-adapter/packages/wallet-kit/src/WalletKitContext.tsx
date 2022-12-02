import {
  createContext,
  ReactNode,
  useContext,
  useMemo,
  useRef,
  useSyncExternalStore,
} from "react";
import { WalletKitCore } from "@mysten/wallet-kit-core";
import { WalletStandardAdapterProvider } from "@mysten/wallet-adapter-wallet-standard";
import { WalletAdapterList } from "@mysten/wallet-adapter-base";

export const WalletKitContext = createContext<WalletKitCore | null>(null);

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

export function useWalletKit() {
  const walletKit = useContext(WalletKitContext);
  if (!walletKit) {
    throw new Error(
      "You must call `useWalletKit` within the of the `WalletKitProvider`."
    );
  }

  return walletKit;
}

// TODO: Should this actually be separate?
export function useWalletKitState() {
  const walletKit = useWalletKit();
  return useSyncExternalStore(walletKit.subscribe, walletKit.getState);
}
