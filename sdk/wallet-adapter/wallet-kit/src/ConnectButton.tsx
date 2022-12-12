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
  const [accountModalOpen, setAccountModalOpen] = useState(false);
  const [connectModalOpen, setConnectModalOpen] = useState(false);
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
          onClick={() => setAccountModalOpen(true)}
          type="button"
          {...props}
        >
          {`${account.slice(0, 4)}...${account.slice(-4)}`}
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

      {account ? (
        <AccountModal
          account={account}
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
