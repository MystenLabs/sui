// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CONSTANTS } from "@/constants";
import { useTransactionExecution } from "@/hooks/useTransactionExecution";
import { useCurrentAccount } from "@mysten/dapp-kit";
import { TransactionBlock } from "@mysten/sui.js/transactions";
import { useMutation, useQueryClient } from "@tanstack/react-query";

// SPDX-License-Identifier: Apache-2.0
export function useGenerateDemoData() {
  const account = useCurrentAccount();
  const executeTransaction = useTransactionExecution();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!account?.address)
        throw new Error("You need to connect your wallet!");
      const txb = new TransactionBlock();

      const bear = txb.moveCall({
        target: `${CONSTANTS.demoContract.packageId}::demo_bear::new`,
        arguments: [txb.pure.string(`A happy bear`)],
      });

      txb.transferObjects([bear], txb.pure.address(account.address));

      return executeTransaction(txb);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["getOwnedObjects"],
      });
    },
  });
}
