import { useWallet } from "@mysten/wallet-adapter-react";
import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useState } from "react";
import { styled } from "./stitches";
import { Button } from "./utils/Button";
import { CloseIcon } from "./utils/Close";
import { WhatIsAWallet } from "./WhatIsAWallet";

// TODO: Ideally remove:
const Div = styled("div");

export interface ConnectModalProps {
  open: boolean;
  onClose(): void;
  closeIcon?: void;
}

const Truncate = styled("div", {
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
});

const Overlay = styled(Dialog.Overlay, {
  backgroundColor: "$backdrop",
  position: "fixed",
  inset: 0,
});

const Content = styled(Dialog.Content, {
  overflow: "hidden",
  backgroundColor: "$background",
  borderRadius: "$modal",
  boxShadow: "$modal",
  position: "fixed",
  top: "50%",
  left: "50%",
  transform: "translate(-50%, -50%)",
  fontFamily: "$sans",
  display: "flex",

  // TODO: Good values for these:
  width: "90vw",
  minHeight: "50vh",
  maxWidth: "700px",
  maxHeight: "85vh",
});

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

const Title = styled(Dialog.Title, {
  margin: 0,
  padding: "0 $2",
  fontSize: "$lg",
  fontWeight: "$title",
  color: "$textDark",
});

const Panel = styled("div", {
  boxSizing: "border-box",
  padding: "$5",
  display: "flex",
  flexDirection: "column",
});

const ConnectWallet = styled(Panel, {
  background: "$backgroundAccent",
  height: "100%",
});

const WalletList = styled("div", {
  marginTop: "$6",
  display: "flex",
  flexDirection: "column",
  gap: "$1",
});

const WalletItem = styled("button", {
  background: "none",
  display: "flex",
  padding: "$2",
  gap: "$2",
  alignItems: "center",
  cursor: "pointer",
  color: "$textDark",
  border: "none",
  fontWeight: "$button",
  fontSize: "$md",
  borderRadius: "$wallet",

  variants: {
    selected: {
      true: {
        background: "$background",
        boxShadow: "$wallet",
      },
    },
  },
});

const WalletIcon = styled("img", {
  flexShrink: 0,
  background: "white",
  width: "$walletIcon",
  height: "$walletIcon",
  borderRadius: "$walletIcon",
  objectFit: "cover",
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

const RetryContainer = styled("div", {
  position: "absolute",
  bottom: "$8",
  right: "$8",
});

export interface ConnectModalProps {
  open: boolean;
  onClose(): void;
}

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
          <Div css={{ width: 240 }}>
            <ConnectWallet>
              <Title>Connect a Wallet</Title>
              <WalletList>
                {wallets.map((wallet) => (
                  <WalletItem
                    key={wallet.name}
                    selected={wallet.name === selected}
                    onClick={() => {
                      setSelected(wallet.name);
                      select(wallet.name);
                    }}
                  >
                    <WalletIcon src={wallet.icon} />
                    <Truncate>{wallet.name}</Truncate>
                  </WalletItem>
                ))}
              </WalletList>
            </ConnectWallet>
          </Div>
          <Panel css={{ flex: 1 }}>
            {selected ? (
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
                  <RetryContainer>
                    <Button color="secondary" onClick={() => select(selected)}>
                      Retry Connection
                    </Button>
                  </RetryContainer>
                )}
              </BodyCopy>
            ) : (
              <>
                <Title css={{ textAlign: "center" }}>What is a Wallet</Title>

                <BodyCopy>
                  <WhatIsAWallet />
                </BodyCopy>
              </>
            )}
          </Panel>

          <Close aria-label="Close">
            <CloseIcon />
          </Close>
        </Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
