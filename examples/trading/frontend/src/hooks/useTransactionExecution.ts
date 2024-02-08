// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useSignTransactionBlock, useSuiClient } from "@mysten/dapp-kit";
import { SuiTransactionBlockResponse } from "@mysten/sui.js/client";
import { TransactionBlock } from "@mysten/sui.js/transactions";
import toast from "react-hot-toast";

export function useTransactionExecution() {
  const client = useSuiClient();
  const { mutateAsync: signTransactionBlock } = useSignTransactionBlock();

  const executeTransaction = async (
    txb: TransactionBlock,
  ): Promise<SuiTransactionBlockResponse | void> => {
    try {
      const signature = await signTransactionBlock({
        transactionBlock: txb,
      });

      const res = await client.executeTransactionBlock({
        transactionBlock: signature.transactionBlockBytes,
        signature: signature.signature,
        options: {
          showEffects: true,
          showObjectChanges: true,
        },
      });

      toast.success("Successfully executed transaction!");
      return res;
    } catch (e: any) {
      toast.error(`Failed to execute transaction: ${e.message as string}`);
    }
  };

  return executeTransaction;
}
