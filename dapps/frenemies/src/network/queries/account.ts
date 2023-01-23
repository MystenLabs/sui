// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { useQuery } from "@tanstack/react-query";

export function useAccount() {
    return useQuery(
        ["account", () => useWalletKit().isConnected],
        async (): Promise<string | null> => {
            const { currentAccount } = useWalletKit();
            return currentAccount;
        }
    )
}
