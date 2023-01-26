// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useQuery } from "@tanstack/react-query";
import { formatBalance } from "../../utils/format";
import provider from "../../network/provider";

export function Balance() {
  const { currentAccount } = useWalletKit();
  const { data } = useQuery(
    ["account", "balance"],
    async () => {
      const [{ decimals }, { totalBalance }] = await Promise.all([
        provider.getCoinMetadata(SUI_TYPE_ARG),
        provider.getBalance(currentAccount!, SUI_TYPE_ARG),
      ]);

      return {
        balance: BigInt(totalBalance),
        decimals,
      };
    },
    {
      enabled: !!currentAccount,
    }
  );

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
