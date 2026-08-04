// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGraphQLClient } from "@mysten/sui/graphql";

declare const senderAddress: string;

const USDC = "0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7::usdc::USDC";

// docs::#query-history
const gqlClient = new SuiGraphQLClient({
    url: "https://sui-testnet.mystenlabs.com/graphql",
    network: "testnet",
});

interface BalanceChange {
    coinType: string;
    owner: string;
    amount: string;
}

interface HistoryQueryResult {
    address?: {
        transactions?: {
            nodes: Array<{
                digest: string;
                effects?: { balanceChangesJson?: BalanceChange[] };
            }>;
        };
    };
}

const { data } = await gqlClient.query<HistoryQueryResult>({
    variables: { sender: senderAddress },
    query: `query AgentHistory($sender: SuiAddress!) {
    address(address: $sender) {
      transactions(last: 20) {
        nodes {
          digest
          effects {
            balanceChangesJson
          }
        }
      }
    }
  }`,
});

for (const txn of data?.address?.transactions?.nodes ?? []) {
    const changes = txn.effects?.balanceChangesJson ?? [];

    // Pair the sender's negative change with the recipient's positive change
    const sent = changes.find(
        (change) =>
            change.coinType === USDC &&
            change.owner === senderAddress &&
            BigInt(change.amount) < 0n,
    );
    const received = changes.find(
        (change) =>
            change.coinType === USDC &&
            change.owner !== senderAddress &&
            BigInt(change.amount) > 0n,
    );

    if (sent && received) {
        console.log(
            `Sent ${Math.abs(Number(sent.amount)) / 1_000_000} USDC`,
            `to ${received.owner}`,
        );
    }
}
// docs::/#query-history
