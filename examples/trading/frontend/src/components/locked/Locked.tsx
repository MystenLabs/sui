// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCurrentAccount, useSuiClientQuery } from "@mysten/dapp-kit";
import { SuiObjectDisplay } from "@/components/SuiObjectDisplay";
import { Button } from "@radix-ui/themes";
import {
  ArrowDownIcon,
  ArrowUpIcon,
  LockOpen1Icon,
} from "@radix-ui/react-icons";
import { ObjectLink } from "../ObjectLink";
import { useState } from "react";
import { LockedObject } from "@/types/types";
import { CreateEscrow } from "../escrows/CreateEscrow";
import { useUnlockMutation } from "@/mutations/locked";

export function Locked({
  locked,
  isManagement,
  hideControls,
}: {
  locked: LockedObject;
  isManagement?: boolean;
  hideControls?: boolean;
}) {
  const [isToggled, setIsToggled] = useState(false);
  const account = useCurrentAccount();
  const { mutate: unlockMutation, isPending } = useUnlockMutation();

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

  const getLabel = () => {
    if (locked.deleted) return "Deleted";
    if (hideControls) {
      if (locked.creator === account?.address) return "You offer this";
      return "You'll receive this if accepted";
    }
    return undefined;
  };

  const getLabelClasses = () => {
    if (locked.deleted)
      return "bg-red-50 rounded px-3 py-1 text-sm text-red-500";
    if (hideControls) {
      if (locked.creator === account?.address)
        return "bg-blue-50 rounded px-3 py-1 text-sm text-blue-500";
      return "bg-green-50 rounded px-3 py-1 text-sm text-green-700";
    }
    return undefined;
  };

  return (
    <div>
      <SuiObjectDisplay
        object={suiObject.data!}
        label={getLabel()}
        labelClasses={getLabelClasses()}
      >
        <div className="text-right flex flex-wrap items-center justify-between">
          {
            <p className="text-sm flex-shrink-0 flex items-center gap-2">
              <ObjectLink id={locked.objectId} isAddress={false} />
            </p>
          }
          {!hideControls && isOwner() && (
            <Button
              className="ml-auto cursor-pointer"
              disabled={isPending}
              onClick={() => {
                unlockMutation({ locked, suiObject: suiObject.data! });
              }}
            >
              <LockOpen1Icon /> Unlock
            </Button>
          )}
          {!isManagement && !isOwner() && (
            <Button
              className="ml-auto cursor-pointer bg-transparent text-black disabled:opacity-40"
              disabled={!account?.address}
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
