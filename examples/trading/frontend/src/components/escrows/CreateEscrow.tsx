// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { LockedObject } from "@/types/types";
import { useCurrentAccount, useSuiClientInfiniteQuery } from "@mysten/dapp-kit";
import { formatAddress } from "@mysten/sui.js/utils";
import { Avatar, Button, Select } from "@radix-ui/themes";
import { InfiniteScrollArea } from "../InfiniteScrollArea";
import { useState } from "react";
import { TransactionBlock } from "@mysten/sui.js/transactions";
import { CONSTANTS } from "@/constants";
import { useTransactionExecution } from "@/hooks/useTransactionExecution";

export function CreateEscrow({ locked }: { locked: LockedObject }) {
  const [objectId, setObjectId] = useState<string | undefined>(undefined);
  const account = useCurrentAccount();
  const executeTransaction = useTransactionExecution();
  const { data, fetchNextPage, isFetchingNextPage, hasNextPage, refetch } =
    useSuiClientInfiniteQuery(
      "getOwnedObjects",
      {
        owner: account?.address!,
        options: {
          showDisplay: true,
          showType: true,
        },
      },
      {
        enabled: !!account,
        select: (data) =>
          data.pages
            .flatMap((page) => page.data)
            .filter(
              (x) => !!x.data?.display && !!x.data?.display?.data?.image_url,
            ),
      },
    );

  const createEscrow = async () => {
    const object = data?.find((x) => x.data?.objectId === objectId);

    if (!object) {
      return;
    }

    console.log({ object, locked });
    const txb = new TransactionBlock();
    txb.moveCall({
      target: `${CONSTANTS.escrowContract.packageId}::shared::create`,
      arguments: [
        txb.object(object.data?.objectId!),
        txb.pure.id(locked.keyId),
        txb.pure.address(locked.creator),
      ],
      typeArguments: [object.data?.type!],
    });

    const res = await executeTransaction(txb);

    if (res) {
      console.log(res);
      refetch();
    }
  };

  return (
    <div className="px-3 py-3  mt-3 rounded">
      <label>Select which object you are putting on escrow:</label>
      <Select.Root value={objectId} onValueChange={setObjectId}>
        <Select.Trigger
          className="h-auto min-h-[25px] w-full mt-3 py-2"
          placeholder="Pick an object"
        />
        <Select.Content className="max-w-[550px] overflow-hidden">
          <Select.Group>
            <Select.Label>Select an Object</Select.Label>

            <InfiniteScrollArea
              loadMore={() => fetchNextPage()}
              hasNextPage={hasNextPage}
              loading={isFetchingNextPage}
              gridClasses="grid-cols-1 gap-2"
            >
              {data?.map((object) => {
                return (
                  <Select.Item
                    key={object.data?.objectId!}
                    value={object.data?.objectId!}
                    className="h-auto w-full data-[state=checked]:bg-blue-50 whitespace-pre-wrap overlfow-hidden break-words hover:bg-blue-50 bg-white text-black cursor-pointer"
                  >
                    <div className="flex items-center break-words">
                      <Avatar
                        size="2"
                        radius="medium"
                        fallback="O"
                        className="mr-3"
                        src={object.data?.display?.data?.image_url!}
                      />
                      <div className="text-xs overflow-ellipsis">
                        {(
                          object.data?.display?.data?.name || "No name"
                        ).substring(0, 100)}
                        <p className="text-gray-600">
                          {formatAddress(object.data?.objectId!)}
                        </p>
                      </div>
                    </div>
                  </Select.Item>
                );
              })}
            </InfiniteScrollArea>
          </Select.Group>
        </Select.Content>
      </Select.Root>
      <div className="mt-6 text-right">
        <Button className="cursor-pointer" onClick={createEscrow}>
          Create Escrow
        </Button>
      </div>
    </div>
  );
}
