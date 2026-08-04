// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from '@mysten/sui/grpc';

const client = new SuiGrpcClient({
	baseUrl: 'https://fullnode.mainnet.sui.io:443',
	network: 'mainnet',
});
const userSuiAddress = '0xUSER_ADDRESS';

// docs::#verify-deposit
// Check balance after webhook notification.
const { balance } = await client.core.getBalance({
	owner: userSuiAddress,
	coinType: '0x2::sui::SUI',
});

console.log('Current balance:', balance.balance);
// docs::/#verify-deposit
