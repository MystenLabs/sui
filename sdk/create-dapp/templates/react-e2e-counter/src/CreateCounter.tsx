import { Transaction } from "@mysten/sui/transactions";
import { Button, Container } from "@radix-ui/themes";
import { useSignAndExecuteTransaction, useSuiClient } from "@mysten/dapp-kit";
import { useNetworkVariable } from "./networkConfig";
import { useState } from "react";
import ClipLoader from "react-spinners/ClipLoader";

export function CreateCounter({
  onCreated,
}: {
  onCreated: (id: string) => void;
}) {
  const counterPackageId = useNetworkVariable("counterPackageId");
  const suiClient = useSuiClient();
  const { mutate: signAndExecute } = useSignAndExecuteTransaction();
  const [waitingForTxn, setWaitingForTxn] = useState(false);

  function create() {
    setWaitingForTxn(true);

    const tx = new Transaction();

    tx.moveCall({
      arguments: [],
      target: `${counterPackageId}::counter::create`,
    });

    signAndExecute(
      {
        transaction: tx,
      },
      {
        onSuccess: ({ digest }) => {
          suiClient
            .waitForTransaction({
              digest: digest,
              options: {
                showEffects: true,
              },
            })
            .then((tx) => {
              const objectId = tx.effects?.created?.[0]?.reference?.objectId;

              if (objectId) {
                onCreated(objectId);
              }

              setWaitingForTxn(false);
            });
        },
      },
    );
  }

  return (
    <Container>
      <Button
        size="3"
        onClick={() => {
          create();
        }}
        disabled={waitingForTxn}
      >
        {waitingForTxn ? <ClipLoader size={20} /> : "Create Counter"}
      </Button>
    </Container>
  );
}
