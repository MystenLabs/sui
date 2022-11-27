// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWallet } from "@mysten/wallet-adapter-react";
import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useState } from "react";
import { styled } from "./stitches";
import { Button, Panel } from "./utils/ui";
import { BackIcon, CloseIcon } from "./utils/icons";
import { WhatIsAWallet } from "./WhatIsAWallet";
import { Body, Content, Overlay, Title } from "./utils/Dialog";
import { SELECTED_GETTING_STARTED, WalletList } from "./WalletList";
import { GettingStarted } from "./GettingStarted";

// TODO: Ideally remove:
const Div = styled("div");

export interface ConnectModalProps {
  open: boolean;
  onClose(): void;
  closeIcon?: void;
}

const Close = styled(Dialog.Close, {
  position: "absolute",
  cursor: "pointer",
  padding: 7,
  top: "$4",
  right: "$4",
  display: "flex",
  border: "none",
  alignItems: "center",
  justifyContent: "center",
  color: "$icon",
  backgroundColor: "$backgroundIcon",
  borderRadius: "$close",
});

const BackButton = styled("button", {
  position: "absolute",
  cursor: "pointer",
  top: "$4",
  left: "$4",
  display: "flex",
  border: "none",
  alignItems: "center",
  justifyContent: "center",
  color: "$icon",
  backgroundColor: "transparent",

  "@md": {
    display: "none",
  },
});

const BodyCopy = styled("div", {
  padding: "$10",
  display: "flex",
  flexDirection: "column",
  justifyContent: "center",
  alignItems: "center",
  flex: 1,
});

const SelectedWalletIcon = styled("img", {
  background: "white",
  objectFit: "cover",
  width: 72,
  height: 72,
  borderRadius: 16,
});

const ButtonContainer = styled("div", {
  position: "absolute",
  bottom: "$8",
  right: "$8",
  marginTop: "$4",
});

const LeftPanel = styled("div", {
  display: "flex",
  flexDirection: "column",
  width: "100%",
  "@md": {
    width: 240,
  },

  variants: {
    hasSelected: {
      true: {
        display: "none",
        "@md": {
          display: "block",
        },
      },
    },
  },
});

export interface ConnectModalProps {
  open: boolean;
  onClose(): void;
}

const MobileInfoButton = styled("button", {
  background: "$backgroundAccent",
  textAlign: "center",
  width: "100%",
  padding: "$4",
  border: "none",
  color: "$textLight",
  fontWeight: "$button",
  fontFamily: "$sans",
  cursor: "pointer",

  "@md": {
    display: "none",
  },
});

const SELECTED_INFO = "@@internal/what-is-wallet";

export function ConnectModal({ open, onClose }: ConnectModalProps) {
  const { wallets, select, wallet, connected, isError } = useWallet();
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setSelected(null);
    }
  }, [open]);

  useEffect(() => {
    if (connected && wallet?.name === selected) {
      onClose();
    }
  }, [wallet, selected, connected]);

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(isOpen) => (isOpen ? null : onClose())}
    >
      <Dialog.Portal>
        <Overlay />
        <Content>
          <Body>
            <LeftPanel hasSelected={!!selected}>
              <WalletList
                selected={selected}
                onChange={(walletName) => {
                  setSelected(walletName);
                  select(walletName);
                }}
              />
              <MobileInfoButton onClick={() => setSelected(SELECTED_INFO)}>
                What is a Wallet
              </MobileInfoButton>
            </LeftPanel>

            <Panel responsiveHidden={!selected}>
              <BackButton onClick={() => setSelected(null)} aria-label="Back">
                <BackIcon />
              </BackButton>

              {!selected || selected === SELECTED_INFO ? (
                <>
                  <Title css={{ textAlign: "center" }}>What is a Wallet</Title>

                  <BodyCopy>
                    <WhatIsAWallet />
                  </BodyCopy>
                </>
              ) : selected && selected !== SELECTED_GETTING_STARTED ? (
                <BodyCopy>
                  <SelectedWalletIcon src={wallet?.icon} />
                  <Div
                    css={{
                      marginTop: "$3",
                      marginBottom: "$1",
                      color: "$textDark",
                      fontSize: "$xl",
                      fontWeight: "$title",
                    }}
                  >
                    Opening {selected}
                  </Div>
                  <Div
                    css={{
                      color: isError ? "$issue" : "$textLight",
                      fontSize: "$xs",
                    }}
                  >
                    {isError
                      ? "Connection failed"
                      : "Confirm connection in the wallet..."}
                  </Div>

                  {isError && (
                    <ButtonContainer>
                      <Button
                        color="secondary"
                        onClick={() => select(selected)}
                      >
                        Retry Connection
                      </Button>
                    </ButtonContainer>
                  )}
                </BodyCopy>
              ) : (
                <>
                  <Title css={{ textAlign: "center" }}>
                    Get Started with Sui
                  </Title>

                  <BodyCopy>
                    <GettingStarted />
                    <ButtonContainer>
                      <Button
                        as="a"
                        color="secondary"
                        href="https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil"
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        Install Wallet Extension
                      </Button>
                    </ButtonContainer>
                  </BodyCopy>
                </>
              )}
            </Panel>

            <Close aria-label="Close">
              <CloseIcon />
            </Close>
          </Body>
        </Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
