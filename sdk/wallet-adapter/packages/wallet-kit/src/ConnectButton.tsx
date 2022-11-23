import * as Dialog from "@radix-ui/react-dialog";
import { ComponentProps, ReactNode, useEffect, useState } from "react";
import { styled, theme } from "./stitches";

import { ConnectModal } from "./ConnectModal";
import { useWallet } from "@mysten/wallet-adapter-react";

const Button = styled("button", {
  cursor: "pointer",
  border: "none",
  borderRadius: "$button",
  fontFamily: "$sans",
  fontWeight: "$button",
  fontSize: "$sm",

  variants: {
    size: {
      md: {
        padding: "$2 $4",
      },
      lg: {
        padding: "$3 $5",
      },
    },
    color: {
      primary: {
        backgroundColor: "$brand",
        color: "$textOnBrand",
        "&:hover": {
          backgroundColor: "$brandDark",
        },
      },
      secondary: {
        backgroundColor: "transparent",
        border: "1px solid $secondary",
        color: "$secondaryDark",
      },
      connected: {
        backgroundColor: "$background",
        color: "$textDark",
      },
    },
  },
  defaultVariants: {
    size: "md",
  },
});

interface ConnectButtonProps extends ComponentProps<typeof Button> {
  connectText?: ReactNode;
}

export function ConnectButton({
  connectText = "Connect Wallet",
  ...props
}: ConnectButtonProps) {
  const { connected, getAccounts } = useWallet();
  const [account, setAccount] = useState<string | null>(null);

  useEffect(() => {
    if (!connected) return;

    getAccounts()
      .then((accounts) => setAccount(accounts[0]))
      .catch((e) => {
        console.warn("Error getting accounts");
      });
  }, [connected]);

  if (account) {
    const truncatedAddress = `${account.slice(0, 4)}...${account.slice(-4)}`;
    return (
      <Button color="connected" size="lg">
        {truncatedAddress}
      </Button>
    );
  }

  return (
    <Dialog.Root>
      <div className={theme}>
        <Dialog.Trigger asChild>
          <Button color="primary" size="lg" {...props}>
            {connectText}
          </Button>
        </Dialog.Trigger>

        <ConnectModal />
      </div>
    </Dialog.Root>
  );
}
