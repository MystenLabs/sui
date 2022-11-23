import * as Dialog from "@radix-ui/react-dialog";
import { ComponentProps, ReactNode } from "react";
import { styled, theme } from "./stitches";

import { ConnectModal } from "./ConnectModal";

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
        borderWidth: 1,
        borderColor: "$secondary",
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

export function ConnectedButton() {
  const address = "damir.sui";
  const balance = "0 SUI";

  return (
    <button className="border-none p-0 bg-white rounded-xl text-sui-grey-100 gap-2 flex items-center font-system shadow-xl">
      <div className="pl-4">{balance}</div>
      <div className="m-0.5 p-2 bg-[#f0f0f0] rounded-[11px]">{address}</div>
    </button>
  );
}

interface ConnectButtonProps extends ComponentProps<typeof Button> {
  connectText?: ReactNode;
}

export function ConnectButton({
  connectText = "Connect Wallet",
  ...props
}: ConnectButtonProps) {
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
