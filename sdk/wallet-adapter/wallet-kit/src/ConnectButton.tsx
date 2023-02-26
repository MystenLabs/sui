// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ComponentProps, ReactNode, useState } from "react";

import { ConnectModal } from "./ConnectModal";
import { Button } from "./utils/ui";
import { AccountModal } from "./AccountModal";
import { useWalletKit } from "./WalletKitContext";
import { formatAddress } from "@mysten/sui.js";

interface ConnectButtonProps extends ComponentProps<typeof Button> {
  connectText?: ReactNode;
  loginedText?: string;
  loginedFallback?: (e: any) => void;
  disconnectFallback?: () => void;
}

export function ConnectButton({
  connectText = "Connect Wallet",
  loginedText,
  loginedFallback,
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
          {loginedText ?? formatAddress(currentAccount.address)}
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
        />
      ) : (
        <ConnectModal
          open={connectModalOpen}
          onClose={() => setConnectModalOpen(false)}
        />
      )}
    </>
  );
}
