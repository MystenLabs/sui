// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ComponentProps, ReactNode, useState } from "react";

import { ConnectModal } from "./ConnectModal";
import { Button } from "./utils/ui";
import { AccountModal } from "./AccountModal";
import { useWalletKit } from "./WalletKitContext";
import { formatAddress } from "@mysten/sui.js";
import { WalletAdapter } from "@mysten/wallet-adapter-base/src/index";

interface ConnectButtonProps extends ComponentProps<typeof Button> {
  connectText?: ReactNode;
  connectedText?: string;
  connectedFallback?: (
    wallet: WalletAdapter | null,
    selected: string | null
  ) => void;
  disconnectFallback?: () => void;
}

export function ConnectButton({
  connectText = "Connect Wallet",
  connectedText,
  connectedFallback,
  disconnectFallback,
  ...props
}: ConnectButtonProps) {
  const [accountModalOpen, setAccountModalOpen] = useState(false);
  const [connectModalOpen, setConnectModalOpen] = useState(false);
  const { currentAccount } = useWalletKit();

  return (
    <>
      {currentAccount ? (
        <Button
          css={{ fontFamily: "$mono" }}
          color="connected"
          size="lg"
          onClick={() => setAccountModalOpen(true)}
          type="button"
          {...props}
        >
          {connectedText ?? formatAddress(currentAccount.address)}
        </Button>
      ) : (
        <Button
          color="primary"
          size="lg"
          onClick={() => setConnectModalOpen(true)}
          type="button"
          {...props}
        >
          {connectText}
        </Button>
      )}

      {currentAccount ? (
        <AccountModal
          open={accountModalOpen}
          onClose={() => setAccountModalOpen(false)}
          disconnectFallback={disconnectFallback}
        />
      ) : (
        <ConnectModal
          open={connectModalOpen}
          onClose={() => setConnectModalOpen(false)}
          connectedFallback={connectedFallback}
        />
      )}
    </>
  );
}
