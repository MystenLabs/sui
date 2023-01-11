// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Dialog } from '@headlessui/react';
import { styled } from "./stitches";
import { Content, Overlay, Body, CloseButton } from "./utils/Dialog";
import { Button } from "./utils/ui";
import { useWalletKit } from "./WalletKitContext";

interface AccountModalProps {
  open: boolean;
  onClose(): void;
}

const Account = styled("div", {
  textAlign: "center",
  fontSize: "$lg",
  color: "$textDark",
  fontWeight: "$title",
  padding: "$6 $4",
});

const ButtonGroup = styled("div", {
  display: "flex",
  gap: "$2",
  width: "100%",
});

export function AccountModal({ open, onClose }: AccountModalProps) {
  const { disconnect, currentAccount } = useWalletKit();
  const account = currentAccount || "";

  return (
    <Dialog
      as="div"
      open={open}
      onClose={onClose}
    >
        <Overlay />
        <Content>
          <Body css={{ padding: "$4", minWidth: "320px" }}>
            <Account title={account}>
              {account.slice(0, 4)}...{account.slice(-4)}
            </Account>

            <ButtonGroup>
              <Button
                css={{ flex: 1 }}
                color="secondary"
                onClick={() => navigator.clipboard.writeText(account)}
              >
                Copy Address
              </Button>
              <Button
                css={{ flex: 1 }}
                color="secondary"
                onClick={() => {
                  disconnect();
                  onClose();
                }}
              >
                Disconnect
              </Button>
            </ButtonGroup>

            <CloseButton onClick={onClose} />
          </Body>
        </Content>
    </Dialog>
  );
}
