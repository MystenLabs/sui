// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CONSTANTS, QueryKey } from "@/constants";
import { useTransactionExecution } from "@/hooks/useTransactionExecution";
import { useCurrentAccount } from "@mysten/dapp-kit";
import { Transaction } from "@mysten/sui/transactions";
import { useMutation, useQueryClient } from "@tanstack/react-query";

/**
 * A mutation to generate demo data as part of our demo.
 */
export function useGenerateDemoData() {
  const account = useCurrentAccount();
  const executeTransaction = useTransactionExecution();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!account?.address)
        throw new Error("You need to connect your wallet!");
      const txb = new Transaction();

      const bear = txb.moveCall({
        target: `${CONSTANTS.demoContract.packageId}::demo_bear::new`,
        arguments: [txb.pure.string(`A happy bear`)],
      });

      txb.transferObjects([bear], txb.pure.address(account.address));

      return executeTransaction(txb);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: [QueryKey.GetOwnedObjects],
      });
    },
  });
}
