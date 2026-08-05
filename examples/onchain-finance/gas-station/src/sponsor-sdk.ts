// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	createSponsor,
	defaults,
	gasBudget,
	allowedFunctions,
} from '@mysten-incubation/sponsor';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

const client = new SuiGrpcClient({
	network: 'testnet',
	baseUrl: 'https://fullnode.testnet.sui.io:443',
});
const sponsorKeypair = Ed25519Keypair.fromSecretKey(process.env.SPONSOR_SECRET_KEY!);

// docs::#sponsor-sdk-create
const sponsor = createSponsor({
	signer: sponsorKeypair,
	client,
	validate: [
		defaults(),
		gasBudget({ max: 50_000_000n }),
		allowedFunctions(['0xPACKAGE::module::function']),
	],
});
// docs::/#sponsor-sdk-create

// docs::#sponsor-sdk-sponsor
// The client sends already-built transaction bytes plus their signature.
// signAndExecuteTransaction returns a discriminated union with three outcomes.
const result = await sponsor.signAndExecuteTransaction({
	transaction: txBytes,
	userSignature,
});

if (result.$kind === 'Rejected') {
	// Validation policy declined. No execution occurred, no gas charged.
	console.error('Rejected:', result.Rejected.reason);
} else if (result.$kind === 'FailedTransaction') {
	// Executed onchain but Move execution aborted. Sponsor still pays gas.
	// Do NOT retry: the transaction has a digest and effects.
	console.error('Failed:', result.FailedTransaction.effects.status.error);
} else {
	// Successful execution with intended effects.
	console.log('Success:', result.Transaction.digest);
}
// docs::/#sponsor-sdk-sponsor

// docs::#sponsor-sdk-validate
// defaults() bundles eight built-in validators:
//   validSender, onlyAddressBalanceGas, gasCoinNotUsed, onlySenderWithdrawals,
//   userSignatureMatchesSender, gasBudget, simulationSucceeds, boundedExpiration
//
// Layer additional validators on top of defaults() for your application:
const strictSponsor = createSponsor({
	signer: sponsorKeypair,
	client,
	validate: [
		defaults(),
		gasBudget({ max: 10_000_000n }),
		allowedFunctions([
			'0xPACKAGE::shop::buy',
			'0xPACKAGE::shop::list_item',
		]),
	],
});
// docs::/#sponsor-sdk-validate

declare const txBytes: Uint8Array;
declare const userSignature: string;
export { sponsor, strictSponsor };
