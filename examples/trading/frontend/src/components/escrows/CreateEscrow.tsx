// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ApiLockedObject } from "@/types/types";
import { useCurrentAccount, useSuiClientInfiniteQuery } from "@mysten/dapp-kit";
import { formatAddress } from "@mysten/sui/utils";
import { Avatar, Button, Select } from "@radix-ui/themes";
import { InfiniteScrollArea } from "@/components/InfiniteScrollArea";
import { useState } from "react";
import { ExplorerLink } from "../ExplorerLink";
import { useCreateEscrowMutation } from "@/mutations/escrow";

/**
 * A component that allows the user to create an escrow for a locked object.
 * It fetches all the objects owned by the connected wallet address and allows the user to
 * select one to put on escrow.
 */
export function CreateEscrow({ locked }: { locked: ApiLockedObject }) {
  const [objectId, setObjectId] = useState<string | undefined>(undefined);
  const account = useCurrentAccount();

  const { mutate: createEscrowMutation, isPending } = useCreateEscrowMutation();

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
              // we're filtering out objects that don't have Display or image_url
              // for demo purposes. The Escrow contract works with all objects.
              (x) => !!x.data?.display && !!x.data?.display?.data?.image_url,
            ),
      },
    );

  const getObject = () => {
    const object = data?.find((x) => x.data?.objectId === objectId);

    if (!object || !object.data) {
      return;
    }
    return object.data;
  };

  return (
    <div className="px-3 py-3 grid grid-cols-1 gap-5 mt-3 rounded">
      <div>
        <label className="text-xs">The recipient will be:</label>
        <ExplorerLink id={locked.creator!} isAddress />
      </div>
      <div>
        <label className="text-xs">Select which object to put on escrow:</label>
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
                      className="h-auto w-full data-[state=checked]:bg-blue-50 whitespace-pre-wrap overflow-hidden break-words hover:bg-blue-50 bg-white text-black cursor-pointer"
                    >
                      <div className="flex items-center break-words">
                        <Avatar
                          size="2"
                          radius="medium"
                          fallback="*"
                          className="mr-3"
                          src={object.data?.display?.data?.image_url!}
                        />
                        <div className="text-xs overflow-ellipsis">
                          {(object.data?.display?.data?.name || "-").substring(
                            0,
                            100,
                          )}
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
      </div>
      {objectId && (
        <div>
          <label className="text-xs">You'll be offering:</label>
          <ExplorerLink id={objectId} />
        </div>
      )}
      <div className="text-right">
        <Button
          className="cursor-pointer"
          disabled={isPending || !objectId}
          onClick={() => {
            createEscrowMutation(
              { locked, object: getObject()! },
              {
                onSuccess: () => {
                  refetch();
                  setObjectId(undefined);
                },
              },
            );
          }}
        >
          Create Escrow
        </Button>
      </div>
    </div>
  );
}
