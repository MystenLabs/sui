// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCurrentAccount, useSuiClientInfiniteQuery } from "@mysten/dapp-kit";
import { SuiObjectDisplay } from "@/components/SuiObjectDisplay";
import { Button } from "@radix-ui/themes";
import { TransactionBlock } from "@mysten/sui.js/transactions";
import { useTransactionExecution } from "@/hooks/useTransactionExecution";
import { CONSTANTS } from "@/constants";
import { SuiObjectData } from "@mysten/sui.js/client";
import { LockClosedIcon } from "@radix-ui/react-icons";
import { InfiniteScrollArea } from "@/components/InfiniteScrollArea";

export function LockItems() {
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

  const lockItem = async (object: SuiObjectData) => {
    if (!object || !("type" in object) || !object.type || !account?.address)
      return;
    const txb = new TransactionBlock();

    const [locked, key] = txb.moveCall({
      target: `${CONSTANTS.escrowContract.packageId}::lock::lock`,
      arguments: [txb.object(object.objectId)],
      typeArguments: [object.type],
    });

    txb.transferObjects([locked, key], account.address);

    const res = await executeTransaction(txb);
    if (res) refetch();
  };

  return (
    <InfiniteScrollArea
      loadMore={() => fetchNextPage()}
      hasNextPage={hasNextPage}
      loading={isFetchingNextPage}
    >
      <>
        {data?.map((obj) => (
          <SuiObjectDisplay object={obj.data!}>
            <div className="text-right flex items-center justify-between">
              <p className="text-sm">
                Lock the item so it can be used for escrows.
              </p>
              <Button
                className="cursor-pointer"
                onClick={() => lockItem(obj.data!)}
              >
                <LockClosedIcon />
                Lock Item
              </Button>
            </div>
          </SuiObjectDisplay>
        ))}
      </>
    </InfiniteScrollArea>
  );
}
