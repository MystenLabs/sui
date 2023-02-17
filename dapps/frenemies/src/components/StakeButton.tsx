// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, VariantProps } from "class-variance-authority";
import { ReactNode } from "react";

const buttonStyles = cva(["py-1 w-20 text-body font-semibold rounded"], {
  variants: {
    disabled: {
      true: "text-steel-dark bg-gradient-to-b from-[#D0E8EF] to-[#B9DAE4] opacity-60",
      false: "bg-frenemies text-white",
    },
  },
});

interface Props extends VariantProps<typeof buttonStyles> {
  children: ReactNode;
  disabled?: boolean;
  onClick(): void;
}

export function StakeButton({ children, disabled = false, onClick }: Props) {
  return (
    <div className="absolute right-0 mr-2">
      <button
        className={buttonStyles({ disabled })}
        disabled={disabled}
        onClick={onClick}
      >
        {children}
      </button>
    </div>
  );
}
