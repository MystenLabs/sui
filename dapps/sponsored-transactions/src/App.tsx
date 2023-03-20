// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  SignedTransaction,
  SuiTransactionResponse,
  Transaction,
} from "@mysten/sui.js";
import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { ComponentProps, ReactNode, useEffect, useState } from "react";
import { provider } from "./utils/rpc";
import { sponsorTransaction } from "./utils/sponsorTransaction";

const tx = new Transaction();
tx.moveCall({
  target: "0x2::devnet_nft::mint",
  arguments: [tx.pure("foo"), tx.pure("bar"), tx.pure("baz")],
});

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
  json: object | null;
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
  const { currentAccount, signTransaction } = useWalletKit();
  const [loading, setLoading] = useState(false);
  const [sponsoredTx, setSponsoredTx] = useState<SignedTransaction | null>(
    null
  );
  const [signedTx, setSignedTx] = useState<SignedTransaction | null>(null);
  const [executedTx, setExecutedTx] = useState<SuiTransactionResponse | null>(
    null
  );

  return (
    <div className="p-8">
      <div className="grid grid-cols-4 gap-8">
        <CodePanel
          title="Transaction details"
          json={tx.transactionData}
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
                  const bytes = await tx.build({
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
                  const signed = await signTransaction({
                    transaction: Transaction.from(
                      sponsoredTx!.transactionBytes
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
                  const executed = await provider.executeTransaction({
                    transaction: signedTx!.transactionBytes,
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
