// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ComponentProps, ReactNode, useEffect, useState } from "react";

import { ConnectModal } from "./ConnectModal";
import { Button } from "./utils/ui";
import { AccountModal } from "./AccountModal";
import { useWalletKitState } from "./WalletKitContext";

interface ConnectButtonProps extends ComponentProps<typeof Button> {
  connectText?: ReactNode;
}

export function ConnectButton({
  connectText = "Connect Wallet",
  ...props
}: ConnectButtonProps) {
  const [open, setOpen] = useState(false);
  const { currentAccount } = useWalletKitState();

  return (
    <>
      {currentAccount ? (
        <Button
          css={{ fontFamily: "$mono" }}
          color="connected"
          size="lg"
          onClick={() => setOpen(true)}
          {...props}
        >
          {`${currentAccount.slice(0, 4)}...${currentAccount.slice(-4)}`}
        </Button>
      ) : (
        <Button
          color="primary"
          size="lg"
          onClick={() => setOpen(true)}
          {...props}
        >
          {connectText}
        </Button>
      )}

      {currentAccount ? (
        <AccountModal open={open} onClose={() => setOpen(false)} />
      ) : (
        <ConnectModal open={open} onClose={() => setOpen(false)} />
      )}
    </>
  );
}
