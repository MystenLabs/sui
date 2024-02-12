// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CONSTANTS } from "@/constants";
import { useSuiClientQuery } from "@mysten/dapp-kit";
import { Locked } from "./Locked";
import { SuiObjectData } from "@mysten/sui.js/client";

export function LockedObject({ object }: { object: SuiObjectData }) {
  const owner = () => {
    if (
      !object.owner ||
      typeof object.owner === "string" ||
      !("AddressOwner" in object.owner)
    )
      return undefined;
    return object.owner.AddressOwner;
  };

  const getKeyId = (item: SuiObjectData) => {
    if (
      !(item.content?.dataType === "moveObject") ||
      !("key" in item.content.fields)
    )
      return "";
    return item.content.fields.key as string;
  };

  // Get the itemID for the locked object (We've saved it as a DOF on the SC).
  const suiObjectId = useSuiClientQuery(
    "getDynamicFieldObject",
    {
      parentId: object.objectId,
      name: {
        type: CONSTANTS.escrowContract.lockedObjectDFKey,
        value: {
          dummy_field: false,
        },
      },
    },
    {
      select: (data) => data.data,
    },
  );

  return (
    <Locked
      locked={{
        id: "-1",
        itemId: suiObjectId.data?.objectId!,
        objectId: object.objectId,
        keyId: getKeyId(object),
        owner: owner(),
        deleted: false,
      }}
    />
  );
}
