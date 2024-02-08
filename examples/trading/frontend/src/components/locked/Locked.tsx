// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  useCurrentAccount,
  useSuiClient,
  useSuiClientQuery,
} from "@mysten/dapp-kit";
import { SuiObjectDisplay } from "@/components/SuiObjectDisplay";
import { Button } from "@radix-ui/themes";
import {
  ArrowDownIcon,
  ArrowUpIcon,
  LockOpen1Icon,
} from "@radix-ui/react-icons";
import toast from "react-hot-toast";
import { TransactionBlock } from "@mysten/sui.js/transactions";
import { CONSTANTS, QueryKey } from "@/constants";
import { useTransactionExecution } from "@/hooks/useTransactionExecution";
import { ObjectLink } from "../ObjectLink";
import { useState } from "react";
import { LockedObject } from "@/types/types";
import { CreateEscrow } from "../escrows/CreateEscrow";
import { useQueryClient } from "@tanstack/react-query";

export function Locked({
  locked,
  isManagement,
}: {
  locked: LockedObject;
  isManagement?: boolean;
}) {
  const [isToggled, setIsToggled] = useState(false);

  const account = useCurrentAccount();
  const client = useSuiClient();
  const executeTransaction = useTransactionExecution();
  const queryClient = useQueryClient();

  const suiObject = useSuiClientQuery(
    "getObject",
    {
      id: locked.itemId,
      options: {
        showDisplay: true,
        showType: true,
        showOwner: true,
      },
    },
    {
      select: (data) => data.data,
    },
  );

  const isOwner = () => {
    return account?.address === locked.creator;
  };

  const unlock = async () => {
    if (!account?.address) return;
    const key = await client.getObject({
      id: locked.keyId,
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

    const txb = new TransactionBlock();

    const item = txb.moveCall({
      target: `${CONSTANTS.escrowContract.packageId}::lock::unlock`,
      typeArguments: [suiObject.data?.type!],
      arguments: [txb.object(locked.objectId), txb.object(locked.keyId)],
    });

    txb.transferObjects([item], txb.pure.address(account.address));

    const res = await executeTransaction(txb);

    if (res) {
      setTimeout(() => {
        // invalidating the queries after a small latency
        // because the indexer works in intervals of 1s.
        // if we invalidate too early, we might not get the latest state.
        queryClient.invalidateQueries({
          queryKey: [QueryKey.Locked],
        });
      }, 1_000);
    }
  };

  return (
    <div>
      <SuiObjectDisplay object={suiObject.data!}>
        <div className="text-right flex flex-wrap items-center justify-between">
          {
            <p className="text-sm flex-shrink-0 flex items-center gap-2">
              <ObjectLink id={locked.objectId} isAddress={false} />
            </p>
          }
          {isOwner() && (
            <Button className="ml-auto cursor-pointer" onClick={unlock}>
              <LockOpen1Icon /> Unlock
            </Button>
          )}
          {!isManagement && !isOwner() && (
            <Button
              className="ml-auto cursor-pointer bg-transparent text-black"
              onClick={() => setIsToggled(!isToggled)}
            >
              Start Escrow
              {isToggled ? <ArrowUpIcon /> : <ArrowDownIcon />}
            </Button>
          )}
          {isToggled && (
            <div className="min-w-[340px] w-full justify-self-start text-left">
              <CreateEscrow locked={locked} />
            </div>
          )}
        </div>
      </SuiObjectDisplay>
    </div>
  );
}
