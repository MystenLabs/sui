// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ConnectButton } from "@mysten/wallet-kit";
import { useEffect, useState } from "react";
import clsx from "clsx";

export function Header() {
  const [scrolled, setScrolled] = useState(false);

  // TODO: Probably debounce this:
  useEffect(() => {
    const listener = () => setScrolled(window.pageYOffset > 40);
    window.addEventListener("scroll", listener, { passive: true });
    return () => window.removeEventListener("scroll", listener);
  }, []);

  return (
    <header
      className={clsx(
        "py-4 px-8 flex items-center sticky top-0 transition-all",
        scrolled ? "backdrop-blur-xl bg-white/70" : "bg-white/0"
      )}
    >
      <div className="flex-1 flex items-center gap-2.5">
        <img src="/sui.svg" alt="Sui Logo" />
        <div className="text-2xl font-semibold text-hero-darkest leading-tight">
          Frenemies Staking Game
        </div>
      </div>

      <div className="">
        <ConnectButton className="!bg-white !text-steel-dark !px-5 !py-3 leading-none" />
      </div>
    </header>
  );
}
