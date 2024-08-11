// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { tmpdir } from 'os';
import path from 'path';
import { fromB64 } from '@mysten/bcs';
import { describe, expect, it } from 'vitest';

import { decodeSuiPrivateKey } from '../../src/cryptography';
import { Ed25519Keypair } from '../../src/keypairs/ed25519';
import { MultiSigPublicKey } from '../../src/multisig/publickey';
import { Transaction } from '../../src/transactions';
import { getZkLoginSignature } from '../../src/zklogin';
import { toZkLoginPublicIdentifier } from '../../src/zklogin/publickey';
import { DEFAULT_RECIPIENT, setupWithFundedAddress } from './utils/setup';

describe('MultiSig with zklogin signature', () => {
	it('Execute tx with multisig with 1 sig and 1 zkLogin sig combined', async () => {
		// default ephemeral keypair, address_seed and zklogin inputs defined: https://github.com/MystenLabs/sui/blob/071a2955f7dbb83ee01c35d3a4257926a50a35f5/crates/sui-types/src/unit_tests/zklogin_test_vectors.json
		// set up default zklogin public identifier with address seed consistent with default zklogin proof.
		let pkZklogin = toZkLoginPublicIdentifier(
			BigInt('2455937816256448139232531453880118833510874847675649348355284726183344259587'),
			'https://id.twitch.tv/oauth2',
		);
		// set up ephemeral keypair, consistent with default zklogin proof.
		let parsed = decodeSuiPrivateKey(
			'suiprivkey1qzdlfxn2qa2lj5uprl8pyhexs02sg2wrhdy7qaq50cqgnffw4c2477kg9h3',
		);
		let ephemeralKeypair = Ed25519Keypair.fromSecretKey(parsed.secretKey);

		// set up default single keypair.
		let kp = Ed25519Keypair.fromSecretKey(
			new Uint8Array([
				126, 57, 195, 235, 248, 196, 105, 68, 115, 164, 8, 221, 100, 250, 137, 160, 245, 43, 220,
				168, 250, 73, 119, 95, 19, 242, 100, 105, 81, 114, 86, 105,
			]),
		);
		let pkSingle = kp.getPublicKey();
		// construct multisig address.
		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 1,
			publicKeys: [
				{ publicKey: pkSingle, weight: 1 },
				{ publicKey: pkZklogin, weight: 1 },
			],
		});
		let multisigAddr = multiSigPublicKey.toSuiAddress();
		const configPath = path.join(tmpdir(), 'client.yaml');
		let toolbox = await setupWithFundedAddress(kp, multisigAddr, configPath);

		// construct a transfer from the multisig address.
		const tx = new Transaction();
		tx.setSenderIfNotSet(multisigAddr);
		const coin = tx.splitCoins(tx.gas, [1]);
		tx.transferObjects([coin], DEFAULT_RECIPIENT);
		let client = toolbox.client;
		let bytes = await tx.build({ client: toolbox.client });

		// sign with the single keypair.
		const singleSig = (await kp.signTransaction(bytes)).signature;

		const zkLoginInputs = {
			addressSeed: '2455937816256448139232531453880118833510874847675649348355284726183344259587',
			headerBase64: 'eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEifQ',
			issBase64Details: {
				indexMod4: 2,
				value: 'wiaXNzIjoiaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyIiw',
			},
			proofPoints: {
				a: [
					'2557188010312611627171871816260238532309920510408732193456156090279866747728',
					'19071990941441318350711693802255556881405833839657840819058116822481115301678',
					'1',
				],
				b: [
					[
						'135230770152349711361478655152288995176559604356405117885164129359471890574',
						'7216898009175721143474942227108999120632545700438440510233575843810308715248',
					],
					[
						'13253503214497870514695718691991905909426624538921072690977377011920360793667',
						'9020530007799152621750172565457249844990381864119377955672172301732296026267',
					],
					['1', '0'],
				],
				c: [
					'873909373264079078688783673576894039693316815418733093168579354008866728804',
					'17533051555163888509441575111667473521314561492884091535743445342304799397998',
					'1',
				],
			},
		};
		const ephemeralSig = (await ephemeralKeypair.signTransaction(bytes)).signature;
		// create zklogin signature based on default zk proof.
		const zkLoginSig = getZkLoginSignature({
			inputs: zkLoginInputs,
			maxEpoch: '2',
			userSignature: fromB64(ephemeralSig),
		});

		// combine to multisig and execute the transaction.
		const signature = multiSigPublicKey.combinePartialSignatures([singleSig, zkLoginSig]);
		let result = await client.executeTransactionBlock({
			transactionBlock: bytes,
			signature,
			options: { showEffects: true },
		});

		// check the execution result and digest.
		const localDigest = await tx.getDigest({ client });
		expect(localDigest).toEqual(result.digest);
		expect(result.effects?.status.status).toEqual('success');
	});
});
