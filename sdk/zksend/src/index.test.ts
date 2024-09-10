// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe } from 'node:test';
import { getFullnodeUrl, SuiClient, SuiObjectChange } from '@mysten/sui/client';
import { decodeSuiPrivateKey, Keypair } from '@mysten/sui/cryptography';
import { getFaucetHost, requestSuiFromFaucetV0 } from '@mysten/sui/faucet';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';
import { toB64 } from '@mysten/sui/utils';
import { beforeAll, expect, test } from 'vitest';

import { getSentTransactionsWithLinks, ZkSendLink, ZkSendLinkBuilder } from './index.js';

export const DEMO_BEAR_CONFIG = {
	packageId: '0xab8ed19f16874f9b8b66b0b6e325ee064848b1a7fdcb1c2f0478b17ad8574e65',
	type: '0xab8ed19f16874f9b8b66b0b6e325ee064848b1a7fdcb1c2f0478b17ad8574e65::demo_bear::DemoBear',
};

export const ZK_BAG_CONFIG = {
	packageId: '0x036fee67274d0d85c3532f58296abe0dee86b93864f1b2b9074be6adb388f138',
	bagStoreId: '0x5c63e71734c82c48a3cb9124c54001d1a09736cfb1668b3b30cd92a96dd4d0ce',
	bagStoreTableId: '0x4e1bc4085d64005e03eb4eab2510d527aeba9548cda431cb8f149ff37451f870',
};

const client = new SuiClient({
	url: getFullnodeUrl('testnet'),
});
const keypair = new Ed25519Keypair();

// Automatically get gas from testnet is not working reliably, manually request gas via discord,
// or uncomment the beforeAll and gas function below
beforeAll(async () => {
	await getSuiFromFaucet(keypair);
});

async function getSuiFromFaucet(keypair: Keypair) {
	const faucetHost = getFaucetHost('testnet');
	const result = await requestSuiFromFaucetV0({
		host: faucetHost,
		recipient: keypair.toSuiAddress(),
	});

	if (result.error) {
		throw new Error(result.error);
	}

	await client.waitForTransaction({
		digest: result.transferredGasObjects[0].transferTxDigest,
	});
}

describe('Contract links', () => {
	test(
		'create and claim link',
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

			link.addClaimableMist(100n);

			const linkUrl = link.getLink();

			await link.create({
				signer: keypair,
				waitForTransaction: true,
			});

			const claimLink = await ZkSendLink.fromUrl(linkUrl, {
				contract: ZK_BAG_CONFIG,
				network: 'testnet',
				claimApi: 'https://zksend-git-mh-contract-claims-mysten-labs.vercel.app/api',
			});

			const claimableAssets = claimLink.assets!;

			expect(claimLink.claimed).toEqual(false);
			expect(claimableAssets.nfts.length).toEqual(3);
			expect(claimableAssets.balances).toMatchInlineSnapshot(`
				[
				  {
				    "amount": 100n,
				    "coinType": "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
				  },
				]
			`);

			const claim = await claimLink.claimAssets(keypair.toSuiAddress());

			const res = await client.waitForTransaction({
				digest: claim.digest,
				options: {
					showObjectChanges: true,
				},
			});

			expect(res.objectChanges?.length).toEqual(
				3 + // bears,
					1 + // coin
					1 + // gas
					1, // bag
			);

			const link2 = await ZkSendLink.fromUrl(linkUrl, {
				contract: ZK_BAG_CONFIG,
				network: 'testnet',
				claimApi: 'https://zksend-git-mh-contract-claims-mysten-labs.vercel.app/api',
			});
			expect(link2.assets?.balances).toEqual(claimLink.assets?.balances);
			expect(link2.assets?.nfts.map((nft) => nft.objectId)).toEqual(
				claimLink.assets?.nfts.map((nft) => nft.objectId),
			);
			expect(link2.claimed).toEqual(true);
		},
		{
			timeout: 30_000,
		},
	);

	test(
		'regenerate links',
		async () => {
			const linkKp = new Ed25519Keypair();

			const link = new ZkSendLinkBuilder({
				keypair: linkKp,
				client,
				contract: ZK_BAG_CONFIG,
				sender: keypair.toSuiAddress(),
			});

			const bears = await createBears(3);

			for (const bear of bears) {
				link.addClaimableObject(bear.objectId);
			}

			link.addClaimableMist(100n);

			const { digest } = await link.create({
				signer: keypair,
				waitForTransaction: true,
			});

			await client.waitForTransaction({ digest });

			const {
				data: [
					{
						links: [lostLink],
					},
				],
			} = await getSentTransactionsWithLinks({
				address: keypair.toSuiAddress(),
				network: 'testnet',
				contract: ZK_BAG_CONFIG,
			});

			const { url, transaction } = await lostLink.createRegenerateTransaction(
				keypair.toSuiAddress(),
			);

			const result = await client.signAndExecuteTransaction({
				transaction,
				signer: keypair,
				options: {
					showEffects: true,
					showObjectChanges: true,
				},
			});

			await client.waitForTransaction({ digest: result.digest });

			const claimLink = await ZkSendLink.fromUrl(url, {
				contract: ZK_BAG_CONFIG,
				network: 'testnet',
				claimApi: 'https://zksend-git-mh-contract-claims-mysten-labs.vercel.app/api',
			});

			expect(claimLink.assets?.nfts.length).toEqual(3);
			expect(claimLink.assets?.balances).toMatchInlineSnapshot(`
				[
				  {
				    "amount": 100n,
				    "coinType": "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
				  },
				]
			`);

			const claim = await claimLink.claimAssets(keypair.toSuiAddress());

			const res = await client.waitForTransaction({
				digest: claim.digest,
				options: {
					showObjectChanges: true,
				},
			});

			expect(res.objectChanges?.length).toEqual(
				3 + // bears,
					1 + // coin
					1 + // gas
					1, // bag
			);
			const link2 = await ZkSendLink.fromUrl(url, {
				contract: ZK_BAG_CONFIG,
				network: 'testnet',
				claimApi: 'https://zksend-git-mh-contract-claims-mysten-labs.vercel.app/api',
			});
			expect(link2.assets?.balances).toEqual(claimLink.assets?.balances);
			expect(link2.assets?.nfts.map((nft) => nft.objectId)).toEqual(
				claimLink.assets?.nfts.map((nft) => nft.objectId),
			);
			expect(link2.claimed).toEqual(true);
		},
		{
			timeout: 30_000,
		},
	);

	test(
		'reclaim links',
		async () => {
			const linkKp = new Ed25519Keypair();

			const link = new ZkSendLinkBuilder({
				keypair: linkKp,
				client,
				contract: ZK_BAG_CONFIG,
				sender: keypair.toSuiAddress(),
			});

			const bears = await createBears(3);

			for (const bear of bears) {
				link.addClaimableObject(bear.objectId);
			}

			link.addClaimableMist(100n);

			const { digest } = await link.create({
				signer: keypair,
				waitForTransaction: true,
			});

			await client.waitForTransaction({ digest });

			const {
				data: [
					{
						links: [lostLink],
					},
				],
			} = await getSentTransactionsWithLinks({
				address: keypair.toSuiAddress(),
				network: 'testnet',
				contract: ZK_BAG_CONFIG,
			});

			const { digest: claimDigest } = await lostLink.claimAssets(keypair.toSuiAddress(), {
				reclaim: true,
				sign: async (tx) => (await keypair.signTransaction(tx)).signature,
			});

			const result = await client.waitForTransaction({
				digest: claimDigest,
				options: { showObjectChanges: true, showEffects: true },
			});

			expect(result.objectChanges?.length).toEqual(
				3 + // bears,
					1 + // coin
					1 + // gas
					1, // bag
			);
		},
		{
			timeout: 30_000,
		},
	);

	test(
		'bulk link creation',
		async () => {
			const bears = await createBears(3);

			const links = [];
			for (const bear of bears) {
				const link = new ZkSendLinkBuilder({
					client,
					contract: ZK_BAG_CONFIG,
					sender: keypair.toSuiAddress(),
				});

				link.addClaimableMist(100n);
				link.addClaimableObject(bear.objectId);

				links.push(link);
			}

			const tx = await ZkSendLinkBuilder.createLinks({
				links,
				client,
				contract: ZK_BAG_CONFIG,
			});

			const result = await client.signAndExecuteTransaction({
				transaction: tx,
				signer: keypair,
			});

			await client.waitForTransaction({ digest: result.digest });

			for (const link of links) {
				const linkUrl = link.getLink();

				const claimLink = await ZkSendLink.fromUrl(linkUrl, {
					contract: ZK_BAG_CONFIG,
					network: 'testnet',
					claimApi: 'https://zksend-git-mh-contract-claims-mysten-labs.vercel.app/api',
				});

				const claimableAssets = claimLink.assets!;

				expect(claimLink.claimed).toEqual(false);
				expect(claimableAssets.nfts.length).toEqual(1);
				expect(claimableAssets.balances).toMatchInlineSnapshot(`
					[
					  {
					    "amount": 100n,
					    "coinType": "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
					  },
					]
				`);

				const claim = await claimLink.claimAssets(keypair.toSuiAddress());

				const res = await client.waitForTransaction({
					digest: claim.digest,
					options: {
						showObjectChanges: true,
					},
				});

				expect(res.objectChanges?.length).toEqual(
					1 + // bears,
						1 + // coin
						1 + // gas
						1, // bag
				);
			}
		},
		{
			timeout: 60_000,
		},
	);
});

describe('Non contract links', () => {
	test(
		'Links with separate gas coin',
		async () => {
			const link = new ZkSendLinkBuilder({
				client,
				sender: keypair.toSuiAddress(),
				contract: null,
			});

			const bears = await createBears(3);

			for (const bear of bears) {
				link.addClaimableObject(bear.objectId);
			}

			link.addClaimableMist(100n);

			const linkUrl = link.getLink();

			await link.create({
				signer: keypair,
				waitForTransaction: true,
			});

			// Balances sometimes not updated even though we wait for the transaction to be indexed
			await new Promise((resolve) => setTimeout(resolve, 3000));

			const claimLink = await ZkSendLink.fromUrl(linkUrl, {
				contract: ZK_BAG_CONFIG,
				network: 'testnet',
			});

			expect(claimLink.assets?.nfts.length).toEqual(3);
			expect(claimLink.assets?.balances).toMatchInlineSnapshot(`
					[
					  {
					    "amount": 100n,
					    "coinType": "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
					  },
					]
				`);

			const claimTx = await claimLink.claimAssets(new Ed25519Keypair().toSuiAddress());

			const res = await client.waitForTransaction({
				digest: claimTx.digest,
				options: {
					showObjectChanges: true,
				},
			});

			expect(res.objectChanges?.length).toEqual(
				3 + // bears,
					1 + // coin
					1, // gas
			);

			const link2 = await ZkSendLink.fromUrl(linkUrl, {
				contract: ZK_BAG_CONFIG,
				network: 'testnet',
				claimApi: 'https://zksend-git-mh-contract-claims-mysten-labs.vercel.app/api',
			});
			expect(link2.assets?.balances).toEqual(claimLink.assets?.balances);
			expect(link2.assets?.nfts.map((nft) => nft.objectId)).toEqual(
				claimLink.assets?.nfts.map((nft) => nft.objectId),
			);
			expect(link2.claimed).toEqual(true);
		},
		{
			timeout: 30_000,
		},
	);

	test(
		'Links with single coin',
		async () => {
			const linkKp = new Ed25519Keypair();

			const tx = new Transaction();

			const [coin] = tx.splitCoins(tx.gas, [5_000_000]);
			tx.transferObjects([coin], linkKp.toSuiAddress());

			const { digest } = await client.signAndExecuteTransaction({
				signer: keypair,
				transaction: tx,
			});

			await client.waitForTransaction({ digest });

			const claimLink = new ZkSendLink({
				keypair: linkKp,
				network: 'testnet',
				isContractLink: false,
			});

			await claimLink.loadAssets();

			expect(claimLink.assets?.nfts.length).toEqual(0);
			expect(claimLink.assets?.balances.length).toEqual(1);
			expect(claimLink.assets?.balances[0].coinType).toEqual(
				'0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI',
			);

			const claimTx = await claimLink.claimAssets(keypair.toSuiAddress());

			const res = await client.waitForTransaction({
				digest: claimTx.digest,
				options: {
					showBalanceChanges: true,
				},
			});

			expect(res.balanceChanges?.length).toEqual(2);
			const link2 = await ZkSendLink.fromUrl(
				`https://zksend.con/claim#${toB64(decodeSuiPrivateKey(linkKp.getSecretKey()).secretKey)}`,
				{
					contract: ZK_BAG_CONFIG,
					network: 'testnet',
					claimApi: 'https://zksend-git-mh-contract-claims-mysten-labs.vercel.app/api',
				},
			);
			expect(link2.assets?.balances).toEqual(claimLink.assets?.balances);
			expect(link2.assets?.nfts.map((nft) => nft.objectId)).toEqual(
				claimLink.assets?.nfts.map((nft) => nft.objectId),
			);
			expect(link2.claimed).toEqual(true);
		},
		{
			timeout: 30_000,
		},
	);

	test(
		'Send to address',
		async () => {
			const link = new ZkSendLinkBuilder({
				client,
				sender: keypair.toSuiAddress(),
				contract: null,
			});

			const bears = await createBears(3);

			for (const bear of bears) {
				link.addClaimableObject(bear.objectId);
			}

			link.addClaimableMist(100n);

			const receiver = new Ed25519Keypair();

			const tx = await link.createSendToAddressTransaction({
				address: receiver.toSuiAddress(),
			});

			const { digest } = await client.signAndExecuteTransaction({
				transaction: tx,
				signer: keypair,
			});

			await client.waitForTransaction({
				digest,
			});

			const objects = await client.getOwnedObjects({
				owner: receiver.toSuiAddress(),
			});

			expect(objects.data.length).toEqual(4);
		},
		{
			timeout: 30_000,
		},
	);

	test(
		'create link with minted assets',
		async () => {
			const link = new ZkSendLinkBuilder({
				client,
				contract: ZK_BAG_CONFIG,
				sender: keypair.toSuiAddress(),
			});

			const tx = new Transaction();

			for (let i = 0; i < 3; i++) {
				const bear = tx.moveCall({
					target: `${DEMO_BEAR_CONFIG.packageId}::demo_bear::new`,
					arguments: [
						tx.pure.string(`A happy bear - ${Math.floor(Math.random() * 1_000_000_000)}`),
					],
				});

				link.addClaimableObjectRef(bear, DEMO_BEAR_CONFIG.type);
			}

			link.addClaimableMist(100n);

			const linkUrl = link.getLink();

			await link.create({
				transaction: tx,
				signer: keypair,
				waitForTransaction: true,
			});

			const claimLink = await ZkSendLink.fromUrl(linkUrl, {
				contract: ZK_BAG_CONFIG,
				network: 'testnet',
				claimApi: 'https://zksend-git-mh-contract-claims-mysten-labs.vercel.app/api',
			});

			const claimableAssets = claimLink.assets!;

			expect(claimLink.claimed).toEqual(false);
			expect(claimableAssets.nfts.length).toEqual(3);
			expect(claimableAssets.balances).toMatchInlineSnapshot(`
				[
				  {
				    "amount": 100n,
				    "coinType": "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
				  },
				]
			`);

			const claim = await claimLink.claimAssets(keypair.toSuiAddress());

			const res = await client.waitForTransaction({
				digest: claim.digest,
				options: {
					showObjectChanges: true,
				},
			});

			expect(res.objectChanges?.length).toEqual(
				3 + // bears,
					1 + // coin
					1 + // gas
					1, // bag
			);

			const link2 = await ZkSendLink.fromUrl(linkUrl, {
				contract: ZK_BAG_CONFIG,
				network: 'testnet',
				claimApi: 'https://zksend-git-mh-contract-claims-mysten-labs.vercel.app/api',
			});
			expect(link2.assets?.balances).toEqual(claimLink.assets?.balances);
			expect(link2.assets?.nfts.map((nft) => nft.objectId).sort()).toEqual(
				claimLink.assets?.nfts.map((nft) => nft.objectId).sort(),
			);
			expect(link2.claimed).toEqual(true);
		},
		{
			timeout: 30_000,
		},
	);
});

async function createBears(totalBears: number) {
	const tx = new Transaction();
	const bears = [];

	for (let i = 0; i < totalBears; i++) {
		const bear = tx.moveCall({
			target: `${DEMO_BEAR_CONFIG.packageId}::demo_bear::new`,
			arguments: [tx.pure.string(`A happy bear - ${Math.floor(Math.random() * 1_000_000_000)}`)],
		});

		bears.push(bear);
	}

	tx.transferObjects(bears, tx.pure.address(keypair.toSuiAddress()));

	const res = await client.signAndExecuteTransaction({
		transaction: tx,
		signer: keypair,
		options: {
			showObjectChanges: true,
		},
	});

	await client.waitForTransaction({
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
