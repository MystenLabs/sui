// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useSignTransaction, useSuiClient } from "@mysten/dapp-kit";
import { SuiTransactionBlockResponse } from "@mysten/sui/client";
import { Transaction } from "@mysten/sui/transactions";
import toast from "react-hot-toast";

/**
 * A hook to execute transactions.
 * It signs the transaction using the wallet and executes it through the RPC.
 *
 * That allows read-after-write consistency and is generally considered a best practice.
 */
export function useTransactionExecution() {
  const client = useSuiClient();
  const { mutateAsync: signTransactionBlock } = useSignTransaction();

  const executeTransaction = async (
    txb: Transaction,
  ): Promise<SuiTransactionBlockResponse | void> => {
    try {
      const signature = await signTransactionBlock({
        transaction: txb,
      });

      const res = await client.executeTransactionBlock({
        transactionBlock: signature.bytes,
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
