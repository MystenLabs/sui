// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient, SuiObjectChange } from '@mysten/sui.js/client';
import { decodeSuiPrivateKey } from '@mysten/sui.js/cryptography';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { describe } from 'node:test';
import { expect, test } from 'vitest';

import { ZkSendLink, ZkSendLinkBuilder } from './links';

export const DEMO_BEAR_CONFIG = {
	packageId: '0xab8ed19f16874f9b8b66b0b6e325ee064848b1a7fdcb1c2f0478b17ad8574e65',
	type: '0xab8ed19f16874f9b8b66b0b6e325ee064848b1a7fdcb1c2f0478b17ad8574e65::demo_bear::DemoBear',
};

export const ZK_BAG_CONFIG = {
	packageId: '0x48c37ed4d37fbbe76af8b6ca29d7bfd80c7c7145bfa4d0b9382cabb5657a70e8',
	bagStoreId: '0xf0f2323539a2097fe8e6aec6be3289ea366375ba0709298957a6a70788d3b955',
	bagStoreTableId: '0xede084021e34f32e80491f9b65c7d4e73cde23f7bc658b57ca1af00d628c9bef',
};

const client = new SuiClient({
	url: getFullnodeUrl('testnet'),
});

const keypair = Ed25519Keypair.fromSecretKey(
	decodeSuiPrivateKey('suiprivkey1qrlgsqryjmmt59nw7a76myeeadxrs3esp8ap2074qz8xaq5kens32f7e3u7')
		.secretKey,
);

// beforeAll(async () => {
// 	// await getSuiFromFaucet(keypair);
// });

describe('Contract links', () => {
	test(
		'create a link',
		async () => {
			const link = new ZkSendLinkBuilder({
				client,
				contract: ZK_BAG_CONFIG,
				sender: keypair.toSuiAddress(),
			});

			const bears = await createBears(3);

			for (const bear of bears) {
				link.addClaimableObject(bear.objectId);
			}

			const linkUrl = link.getLink();

			await link.create({
				signer: keypair,
			});

			const claimLink = await ZkSendLink.fromUrl(linkUrl, {
				contract: ZK_BAG_CONFIG,
				client,
			});

			expect(
				(await claimLink.listClaimableAssets(new Ed25519Keypair().toSuiAddress())).bag.length,
			).toEqual(3);

			const claimTxb = claimLink.createClaimTransaction(keypair.toSuiAddress());

			claimTxb.setGasOwner(keypair.toSuiAddress());

			const claimBytes = await claimTxb.build({
				client,
			});

			const linkSig = await claimLink.keypair.signTransactionBlock(claimBytes);
			const keypairSig = await keypair.signTransactionBlock(claimBytes);

			const res = await client.executeTransactionBlock({
				signature: [linkSig.signature, keypairSig.signature],
				transactionBlock: claimBytes,
				options: {
					showObjectChanges: true,
				},
			});

			console.log(res);
		},
		{
			timeout: 30_000,
		},
	);
});

// async function getSuiFromFaucet(keypair: Keypair) {
// 	const faucetHost = getFaucetHost('testnet');
// 	const result = await requestSuiFromFaucetV0({
// 		host: faucetHost,
// 		recipient: keypair.toSuiAddress(),
// 	});

// 	if (result.error) {
// 		throw new Error(result.error);
// 	}

// 	// let status;
// 	// do {
// 	// 	status = await getFaucetRequestStatus({
// 	// 		host: faucetHost,
// 	// 		taskId: result.task!,
// 	// 	});

// 	// 	if (status.error) {
// 	// 		throw new Error(status.error);
// 	// 	}
// 	// } while (status.status.status === 'INPROGRESS');
// }

async function createBears(totalBears: number) {
	const txb = new TransactionBlock();
	const bears = [];

	for (let i = 0; i < totalBears; i++) {
		const bear = txb.moveCall({
			target: `${DEMO_BEAR_CONFIG.packageId}::demo_bear::new`,
			arguments: [txb.pure.string(`A happy bear - ${Math.floor(Math.random() * 1_000_000_000)}`)],
		});

		bears.push(bear);
	}

	txb.transferObjects(bears, txb.pure.address(keypair.toSuiAddress()));

	const res = await client.signAndExecuteTransactionBlock({
		transactionBlock: txb,
		signer: keypair,
		options: {
			showObjectChanges: true,
		},
	});

	await client.waitForTransactionBlock({
		digest: res.digest,
	});

	const bearList = res
		.objectChanges!.filter(
			(x: SuiObjectChange) => x.type === 'created' && x.objectType.includes(DEMO_BEAR_CONFIG.type),
		)
		.map((x: SuiObjectChange) => {
			if (!('objectId' in x)) throw new Error('invalid data');
			return {
				objectId: x.objectId,
				type: x.objectType,
			};
		});

	return bearList;
}
