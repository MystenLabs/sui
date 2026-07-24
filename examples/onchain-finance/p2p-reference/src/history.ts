// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGraphQLClient } from '@mysten/sui/graphql';

declare const senderAddress: string;

const USDC = '0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7::usdc::USDC';

// docs::#query-history
const gqlClient = new SuiGraphQLClient({
	url: 'https://sui-testnet.mystenlabs.com/graphql',
	network: 'testnet',
});

const { data } = await gqlClient.query({
	query: `{
    address(address: "${senderAddress}") {
      transactions(last: 20) {
        nodes {
          digest
          effects {
            balanceChanges {
              nodes {
                owner { address }
                amount
                coinType { repr }
              }
            }
          }
        }
      }
    }
  }`,
});

for (const txn of data?.address?.transactions?.nodes ?? []) {
	const changes = txn.effects?.balanceChanges?.nodes ?? [];

	// Pair the sender's negative change with the recipient's positive change
	const sent = changes.find(
		(c: any) =>
			c.coinType?.repr === USDC &&
			c.owner?.address === senderAddress &&
			BigInt(c.amount) < 0n,
	);
	const received = changes.find(
		(c: any) =>
			c.coinType?.repr === USDC &&
			c.owner?.address !== senderAddress &&
			BigInt(c.amount) > 0n,
	);

	if (sent && received) {
		console.log(
			`Sent ${Math.abs(Number(sent.amount)) / 1_000_000} USDC`,
			`to ${received.owner?.address}`,
		);
	}
}
// docs::/#query-history
