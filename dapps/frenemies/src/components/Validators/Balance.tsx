// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { formatBalance } from "../../utils/format";
import { useBalance } from "../../network/queries/coin";

export function Balance() {
  const { data } = useBalance();

  return (
    <div className="rounded-full shadow-notification bg-white px-4 py-1 flex items-center gap-11">
      <div>
        <div className="uppercase text-steel text-[10px] leading-tight font-semibold">
          In your wallet
        </div>
        <div className="text-steel-dark">
          <span className="font-semibold">
            {(data && formatBalance(data.balance, data.decimals)) || "--"}
          </span>{" "}
          SUI
        </div>
      </div>
      <div>
        <img src="/sui_icon.svg" />
      </div>
    </div>
  );
}
