import { TransactionBlock } from "@mysten/sui.js/transactions";
import { Button, Container } from "@radix-ui/themes";
import { useCallback } from "react";
import { PACKAGE_ID } from "./constants";
import {
  useSignAndExecuteTransactionBlock,
  useSuiClient,
} from "@mysten/dapp-kit";

export function CreateCounter({
  onCreated,
}: {
  onCreated: (id: string) => void;
}) {
  const suiClient = useSuiClient();
  const { mutateAsync: signAndExecute } = useSignAndExecuteTransactionBlock({});

  const create = useCallback(() => {
    const txb = new TransactionBlock();

    txb.moveCall({
      arguments: [],
      target: `${PACKAGE_ID}::counter::create`,
    });

    signAndExecute({
      requestType: "WaitForEffectsCert",
      transactionBlock: txb,
      options: {
        showEffects: true,
        showObjectChanges: true,
      },
    }).then(async (tx) => {
      await suiClient.waitForTransactionBlock({
        digest: tx.digest,
      });

      const objectId = tx.effects?.created?.[0]?.reference?.objectId;

      if (objectId) {
        onCreated(objectId);
      }
    });
  }, [onCreated, signAndExecute, suiClient]);

  return (
    <Container>
      <Button
        size="3"
        onClick={() => {
          create();
        }}
      >
        Create Counter
      </Button>
    </Container>
  );
}
