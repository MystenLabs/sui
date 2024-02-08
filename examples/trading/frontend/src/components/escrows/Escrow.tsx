// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCurrentAccount, useSuiClientQuery } from "@mysten/dapp-kit";
import { SuiObjectDisplay } from "@/components/SuiObjectDisplay";
import { Button } from "@radix-ui/themes";
import { Cross1Icon } from "@radix-ui/react-icons";
import { useTransactionExecution } from "@/hooks/useTransactionExecution";
import { CONSTANTS } from "@/constants";
import { TransactionBlock } from "@mysten/sui.js/transactions";
import { ObjectLink } from "../ObjectLink";

export type Escrow = {
  id: string;
  objectId: string;
  sender: string;
  recipient: string;
  keyId: string;
  itemId: string;
  swapped: boolean;
  cancelled: boolean;
};
export function Escrow({
  escrow,
  refetch,
}: {
  escrow: Escrow;
  refetch?: () => void;
}) {
  const account = useCurrentAccount();
  const executeTransaction = useTransactionExecution();

  const suiObject = useSuiClientQuery("getObject", {
    id: escrow.itemId,
    options: {
      showDisplay: true,
      showType: true,
    },
  });

  const cancelEscrow = async () => {
    const txb = new TransactionBlock();

    const item = txb.moveCall({
      target: `${CONSTANTS.escrowContract.packageId}::shared::return_to_sender`,
      arguments: [txb.object(escrow.objectId)],
      typeArguments: [suiObject.data?.data?.type!],
    });

    txb.transferObjects([item], txb.pure.address(account?.address!));

    const res = await executeTransaction(txb);

    if (res && refetch) refetch();
  };

  return (
    <div>
      <SuiObjectDisplay object={suiObject.data?.data!}>
        <div className="flex gap-3 flex-wrap justify-between">
          {
            <p className="text-sm flex-shrink-0 flex items-center gap-2">
              <ObjectLink id={escrow.objectId} isAddress={false} />
            </p>
          }
          {!escrow.cancelled &&
            !escrow.swapped &&
            escrow.sender === account?.address && (
              <Button
                color="amber"
                className="cursor-pointer"
                onClick={cancelEscrow}
              >
                <Cross1Icon />
                Cancel escrow
              </Button>
            )}
        </div>
      </SuiObjectDisplay>
    </div>
  );
}
