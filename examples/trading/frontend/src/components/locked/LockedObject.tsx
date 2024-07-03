// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CONSTANTS } from "@/constants";
import { useSuiClientQuery } from "@mysten/dapp-kit";
import { Locked } from "./partials/Locked";
import { SuiObjectData } from "@mysten/sui/client";

/**
 * Acts as a wrapper between the `Locked` object fetched from API
 * and the on-chain object state.
 *
 * Accepts an `object` of type `::locked::Locked`, fetches the itemID (though the DOF)
 * and then renders the `Locked` component.
 *
 * ItemId is optional because we trust the API to return the correct itemId for each Locked.
 */
export function LockedObject({
  object,
  itemId,
  hideControls,
}: {
  object: SuiObjectData;
  itemId?: string;
  hideControls?: boolean;
}) {
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
      enabled: !itemId,
    },
  );

  return (
    <Locked
      locked={{
        itemId: itemId || suiObjectId.data?.objectId!,
        objectId: object.objectId,
        keyId: getKeyId(object),
        creator: owner(),
        deleted: false,
      }}
      hideControls={hideControls}
    />
  );
}
