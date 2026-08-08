// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from '@mysten/sui/grpc';

const client = new SuiGrpcClient({
	baseUrl: 'https://fullnode.testnet.sui.io:443',
	network: 'testnet',
});

const USDC = '0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7::usdc::USDC';

// docs::#read-balance
const { balance } = await client.getBalance({
	owner: senderAddress,
	coinType: USDC,
});

console.log(`Available USDC: ${balance.balance}`);
// docs::/#read-balance

declare const senderAddress: string;
