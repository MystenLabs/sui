import { Button, Flex, Heading, Text } from "@radix-ui/themes";
import {
	useCurrentAccount,
	useSignAndExecuteTransaction,
	useSuiClient,
	useSuiClientQuery,
} from '@mysten/dapp-kit';
import { SuiObjectData } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';

import { COUNTER_PACKAGE_ID } from './constants';

export function Counter({ id }: { id: string }) {
  const { data, refetch } = useSuiClientQuery('getObject', {
    id,
    options: {
      showContent: true,
    },
  });
  const currentAccount = useCurrentAccount();
  const suiClient = useSuiClient();
  const { mutate: signAndExecute } = useSignAndExecuteTransaction({
    execute: async ({ bytes, signature }) =>
      await suiClient.executeTransactionBlock({
        transactionBlock: bytes,
        signature,
        options: {
          // Raw effects are required so the effects can be reported back to the wallet
          showRawEffects: true,
          showEffects: true,
        },
      }),
  });	

  if (!data?.data) return <div>Not found</div>;

  const ownedByCurrentAccount = getCounterFields(data.data)?.owner === currentAccount?.address;

  return (
    <>
      <Heading size="3">Counter {id}</Heading>

      <Flex direction="column" gap="2">
        <Text>Count: {getCounterFields(data.data)?.value}</Text>
        <Flex direction="row" gap="2">
          <Button onClick={() => executeMoveCall("increment")}>
            Increment
          </Button>
          {ownedByCurrentAccount ? (
            <Button onClick={() => executeMoveCall("reset")}>Reset</Button>
          ) : null}
        </Flex>
      </Flex>
    </>
  );

  function executeMoveCall(method: 'increment' | 'reset') {
    const tx = new Transaction();

    if (method === 'reset') {
      tx.moveCall({
        arguments: [tx.object(id), tx.pure.u64(0)],
        target: `${COUNTER_PACKAGE_ID}::counter::set_value`,
      });
    } else {
      tx.moveCall({
        arguments: [tx.object(id)],
        target: `${COUNTER_PACKAGE_ID}::counter::increment`,
      });
    }

    signAndExecute(
      {
        transaction: tx,
      },
      {
        onSuccess: async () => {
          await refetch();
        },
      },
    );
  }
}

function getCounterFields(data: SuiObjectData) {
  if (data.content?.dataType !== 'moveObject') {
    return null;
  }

  return data.content.fields as { value: number; owner: string };
}