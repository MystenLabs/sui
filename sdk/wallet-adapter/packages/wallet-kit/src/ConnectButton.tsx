// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ComponentProps, ReactNode, useEffect, useState } from "react";

import { ConnectModal } from "./ConnectModal";
import { useWallet } from "@mysten/wallet-adapter-react";
import { Button } from "./utils/ui";
import { AccountModal } from "./AccountModal";

interface ConnectButtonProps extends ComponentProps<typeof Button> {
  connectText?: ReactNode;
}

export function ConnectButton({
  connectText = "Connect Wallet",
  ...props
}: ConnectButtonProps) {
  const [open, setOpen] = useState(false);
  const [account, setAccount] = useState<string | null>(null);

  const { connected, getAccounts } = useWallet();

  useEffect(() => {
    if (!connected) {
      setAccount(null);
    } else {
      getAccounts()
        .then((accounts) => setAccount(accounts[0]))
        .catch((e) => {
          console.warn("Error getting accounts");
        });
    }
  }, [connected]);

  return (
    <>
      {account ? (
        <Button
          css={{ fontFamily: "$mono" }}
          color="connected"
          size="lg"
          onClick={() => setOpen(true)}
          {...props}
        >
          {`${account.slice(0, 4)}...${account.slice(-4)}`}
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

      {account ? (
        <AccountModal
          account={account}
          open={open}
          onClose={() => setOpen(false)}
        />
      ) : (
        <ConnectModal open={open} onClose={() => setOpen(false)} />
      )}
    </>
  );
}
