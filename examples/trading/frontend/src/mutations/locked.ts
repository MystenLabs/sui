// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CONSTANTS, QueryKey } from "@/constants";
import { useTransactionExecution } from "@/hooks/useTransactionExecution";
import { useCurrentAccount, useSuiClient } from "@mysten/dapp-kit";
import { SuiObjectData } from "@mysten/sui/client";
import { Transaction } from "@mysten/sui/transactions";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import toast from "react-hot-toast";

//docs::#mutationlock
/**
 * Builds and executes the PTB to lock an object.
 */
export function useLockObjectMutation() {
  const account = useCurrentAccount();
  const executeTransaction = useTransactionExecution();

  return useMutation({
    mutationFn: async ({ object }: { object: SuiObjectData }) => {
      if (!account?.address)
        throw new Error("You need to connect your wallet!");
      const txb = new Transaction();

      const [locked, key] = txb.moveCall({
        target: `${CONSTANTS.escrowContract.packageId}::lock::lock`,
        arguments: [txb.object(object.objectId)],
        typeArguments: [object.type!],
      });

      txb.transferObjects([locked, key], txb.pure.address(account.address));

      return executeTransaction(txb);
    },
  });
}
//docs::/#mutationlock

//docs::#mutationunlock
/**
 * Builds and executes the PTB to unlock an object.
 */
export function useUnlockMutation() {
  const account = useCurrentAccount();
  const executeTransaction = useTransactionExecution();
  const client = useSuiClient();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({
      lockedId,
      keyId,
      suiObject,
    }: {
      lockedId: string;
      keyId: string;
      suiObject: SuiObjectData;
    }) => {
      if (!account?.address)
        throw new Error("You need to connect your wallet!");
      const key = await client.getObject({
        id: keyId,
        options: {
          showOwner: true,
        },
      });

      if (
        !key.data?.owner ||
        typeof key.data.owner === "string" ||
        !("AddressOwner" in key.data.owner) ||
        key.data.owner.AddressOwner !== account.address
      ) {
        toast.error("You are not the owner of the key");
        return;
      }

      const txb = new Transaction();

      const item = txb.moveCall({
        target: `${CONSTANTS.escrowContract.packageId}::lock::unlock`,
        typeArguments: [suiObject.type!],
        arguments: [txb.object(lockedId), txb.object(keyId)],
      });

      txb.transferObjects([item], txb.pure.address(account.address));

      return executeTransaction(txb);
    },
    onSuccess: () => {
      setTimeout(() => {
        // invalidating the queries after a small latency
        // because the indexer works in intervals of 1s.
        // if we invalidate too early, we might not get the latest state.
        queryClient.invalidateQueries({
          queryKey: [QueryKey.Locked],
        });
      }, 1_000);
    },
  });
}
//docs::/#mutationunlock
