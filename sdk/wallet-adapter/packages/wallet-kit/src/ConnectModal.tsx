import { useWallet } from "@mysten/wallet-adapter-react";
import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useState } from "react";
import { styled } from "./stitches";
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
  backgroundColor: "$background",
  borderRadius: "$modal",
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
  fontSize: "$lg",
  fontWeight: "$title",
  color: "$textDark",
});

const ConnectWallet = styled("div", {
  boxSizing: "border-box",
  padding: "$4 $5",
  height: "100%",
  // TODO: Move value:
  background: "#6FBCF01A",
});

const WalletList = styled("div", {
  display: "flex",
  flexDirection: "column",
  gap: "$1",
});

const WalletIcon = styled("img", {
  flexShrink: 0,
  background: "white",
  width: "$walletIcon",
  height: "$walletIcon",
  borderRadius: "$walletIcon",
});

const WalletItem = styled("button", {
  background: "none",
  display: "flex",
  padding: "$1",
  gap: "$2",
  alignItems: "center",
  color: "$textDark",
  border: "none",
  fontWeight: "$button",
  fontSize: "$md",
  borderRadius: "$wallet",

  "&:hover": {
    backgroundColor: "rgba(111, 188, 240, 0.1)",
  },
});

export function ConnectModal() {
  const { wallets, select, wallet } = useWallet();
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    if (selected) {
      select(selected);
    }
  }, [selected]);

  return (
    <Dialog.Portal>
      <Overlay />
      <Content>
        <Close aria-label="Close" />

        <Div css={{ width: 240 }}>
          <ConnectWallet>
            <Title>Connect a Wallet</Title>
            <WalletList>
              {wallets.map((wallet) => (
                <WalletItem
                  key={wallet.name}
                  onClick={() => setSelected(wallet.name)}
                >
                  <WalletIcon src={wallet.icon} />
                  <Truncate>{wallet.name}</Truncate>
                </WalletItem>
              ))}
            </WalletList>
          </ConnectWallet>
        </Div>
        <Div css={{ flex: 1 }}>
          {selected ? (
            <Div
              css={{
                display: "flex",
                flexDirection: "column",
                justifyContent: "center",
                alignItems: "center",
                height: "100%",
              }}
            >
              <Div
                css={{
                  color: "$textDark",
                  fontSize: "$xl",
                  fontWeight: "$title",
                }}
              >
                Opening {selected}
              </Div>
              <Div css={{ color: "$textLight", fontSize: "$xs" }}>
                Confirm connection in the wallet...
              </Div>
            </Div>
          ) : (
            <>
              <Div css={{ display: "flex", justifyContent: "center" }}>
                <Title>What is a Wallet</Title>
              </Div>

              {selected}

              <WhatIsAWallet />
            </>
          )}
        </Div>
      </Content>
    </Dialog.Portal>
  );
}
