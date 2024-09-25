// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { CONSTANTS, QueryKey } from "@/constants";
import { useTransactionExecution } from "@/hooks/useTransactionExecution";
import { ApiEscrowObject, ApiLockedObject } from "@/types/types";
import { useCurrentAccount, useSuiClient } from "@mysten/dapp-kit";
import { SuiObjectData } from "@mysten/sui/client";
import { Transaction } from "@mysten/sui/transactions";
import { useMutation, useQueryClient } from "@tanstack/react-query";

/**
 * Builds and executes the PTB to create an escrow.
 */
export function useCreateEscrowMutation() {
  const currentAccount = useCurrentAccount();
  const executeTransaction = useTransactionExecution();

  return useMutation({
    mutationFn: async ({
      object,
      locked,
    }: {
      object: SuiObjectData;
      locked: ApiLockedObject;
    }) => {
      if (!currentAccount?.address)
        throw new Error("You need to connect your wallet!");

      const txb = new Transaction();
      txb.moveCall({
        target: `${CONSTANTS.escrowContract.packageId}::shared::create`,
        arguments: [
          txb.object(object.objectId!),
          txb.pure.id(locked.keyId),
          txb.pure.address(locked.creator!),
        ],
        typeArguments: [object.type!],
      });

      return executeTransaction(txb);
    },
  });
}

/**
 * Builds and executes the PTB to cancel an escrow.
 */
export function useCancelEscrowMutation() {
  const currentAccount = useCurrentAccount();
  const executeTransaction = useTransactionExecution();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({
      escrow,
      suiObject,
    }: {
      escrow: ApiEscrowObject;
      suiObject: SuiObjectData;
    }) => {
      if (!currentAccount?.address)
        throw new Error("You need to connect your wallet!");
      const txb = new Transaction();

      const item = txb.moveCall({
        target: `${CONSTANTS.escrowContract.packageId}::shared::return_to_sender`,
        arguments: [txb.object(escrow.objectId)],
        typeArguments: [suiObject?.type!],
      });

      txb.transferObjects([item], txb.pure.address(currentAccount?.address!));

      return executeTransaction(txb);
    },

    onSuccess: () => {
      setTimeout(() => {
        queryClient.invalidateQueries({ queryKey: [QueryKey.Escrow] });
      }, 1_000);
    },
  });
}

/**
 * Builds and executes the PTB to accept an escrow.
 */
export function useAcceptEscrowMutation() {
  const currentAccount = useCurrentAccount();
  const client = useSuiClient();
  const executeTransaction = useTransactionExecution();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({
      escrow,
      locked,
    }: {
      escrow: ApiEscrowObject;
      locked: ApiLockedObject;
    }) => {
      if (!currentAccount?.address)
        throw new Error("You need to connect your wallet!");
      const txb = new Transaction();

      const escrowObject = await client.multiGetObjects({
        ids: [escrow.itemId, locked.itemId],
        options: {
          showType: true,
        },
      });

      const escrowType = escrowObject.find(
        (x) => x.data?.objectId === escrow.itemId,
      )?.data?.type;

      const lockedType = escrowObject.find(
        (x) => x.data?.objectId === locked.itemId,
      )?.data?.type;

      if (!escrowType || !lockedType) {
        throw new Error("Failed to fetch types.");
      }

      const item = txb.moveCall({
        target: `${CONSTANTS.escrowContract.packageId}::shared::swap`,
        arguments: [
          txb.object(escrow.objectId),
          txb.object(escrow.keyId),
          txb.object(locked.objectId),
        ],
        typeArguments: [escrowType, lockedType],
      });

      txb.transferObjects([item], txb.pure.address(currentAccount.address));

      return executeTransaction(txb);
    },

    onSuccess: () => {
      setTimeout(() => {
        queryClient.invalidateQueries({ queryKey: [QueryKey.Escrow] });
      }, 1_000);
    },
  });
}
