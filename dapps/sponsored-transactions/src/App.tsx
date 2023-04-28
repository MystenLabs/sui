// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  SignedTransaction,
  SuiTransactionBlockResponse,
  TransactionBlock,
} from "@mysten/sui.js";
import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { ComponentProps, ReactNode, useMemo, useState } from "react";
import { provider } from "./utils/rpc";
import { sponsorTransaction } from "./utils/sponsorTransaction";

const Button = (props: ComponentProps<"button">) => (
  <button
    className="bg-indigo-600 text-sm font-medium text-white rounded-lg px-4 py-3 disabled:cursor-not-allowed disabled:opacity-60"
    {...props}
  />
);

const CodePanel = ({
  title,
  json,
  action,
}: {
  title: string;
  json?: object | null;
  action: ReactNode;
}) => (
  <div>
    <div className="text-lg font-bold mb-2">{title}</div>
    <div className="mb-4">{action}</div>
    <code className="block bg-gray-200 p-2 text-gray-800 rounded text-sm break-all whitespace-pre-wrap">
      {JSON.stringify(json, null, 2)}
    </code>
  </div>
);

export function App() {
  const { currentAccount, signTransactionBlock } = useWalletKit();
  const [loading, setLoading] = useState(false);
  const [sponsoredTx, setSponsoredTx] = useState<SignedTransaction | null>(
    null
  );
  const [signedTx, setSignedTx] = useState<SignedTransaction | null>(null);
  const [executedTx, setExecutedTx] =
    useState<SuiTransactionBlockResponse | null>(null);

  const tx = useMemo(() => {
    if (!currentAccount) return null;
    const tx = new TransactionBlock();
    const [coin] = tx.splitCoins(tx.gas, [tx.pure(1)]);
    tx.transferObjects([coin], tx.pure(currentAccount.address));
    return tx;
  }, [currentAccount]);

  return (
    <div className="p-8">
      <div className="grid grid-cols-4 gap-8">
        <CodePanel
          title="Transaction details"
          json={tx?.blockData}
          action={<ConnectButton className="!bg-indigo-600 !text-white" />}
        />

        <CodePanel
          title="Sponsored Transaction"
          json={sponsoredTx}
          action={
            <Button
              disabled={!currentAccount || loading}
              onClick={async () => {
                setLoading(true);
                try {
                  const bytes = await tx!.build({
                    provider,
                    onlyTransactionKind: true,
                  });
                  const sponsoredBytes = await sponsorTransaction(
                    currentAccount!.address,
                    bytes
                  );
                  setSponsoredTx(sponsoredBytes);
                } finally {
                  setLoading(false);
                }
              }}
            >
              Sponsor Transaction
            </Button>
          }
        />

        <CodePanel
          title="Signed Transaction"
          json={signedTx}
          action={
            <Button
              disabled={!sponsoredTx || loading}
              onClick={async () => {
                setLoading(true);
                try {
                  const signed = await signTransactionBlock({
                    transactionBlock: TransactionBlock.from(
                      sponsoredTx!.transactionBlockBytes
                    ),
                  });
                  setSignedTx(signed);
                } finally {
                  setLoading(false);
                }
              }}
            >
              Sign Transaction
            </Button>
          }
        />
        <CodePanel
          title="Executed Transaction"
          json={executedTx}
          action={
            <Button
              disabled={!signedTx || loading}
              onClick={async () => {
                setLoading(true);
                try {
                  const executed = await provider.executeTransactionBlock({
                    transactionBlock: signedTx!.transactionBlockBytes,
                    signature: [signedTx!.signature, sponsoredTx!.signature],
                    options: {
                      showEffects: true,
                      showEvents: true,
                      showObjectChanges: true,
                    },
                  });
                  setExecutedTx(executed);
                } finally {
                  setLoading(false);
                }
              }}
            >
              Execute Transaction
            </Button>
          }
        />
      </div>
    </div>
  );
}
