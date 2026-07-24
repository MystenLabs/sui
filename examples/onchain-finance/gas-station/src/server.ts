// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import express from 'express';
import { Transaction } from '@mysten/sui/transactions';
import { SuiClient } from '@mysten/sui/client';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { toBase64, fromBase64 } from '@mysten/sui/utils';

import { GasCoinPool } from './pool.js';

// docs::#server-setup
const app = express();
app.use(express.json());

const client = new SuiClient({ url: 'https://fullnode.testnet.sui.io:443' });

// Load sponsor keypair from environment (Bech32 suiprivkey1... string)
const sponsor = Ed25519Keypair.fromSecretKey(process.env.SPONSOR_SECRET_KEY!);
const sponsorAddress = sponsor.toSuiAddress();

// Initialize gas coin pool
const pool = new GasCoinPool();
await pool.initialize(client, sponsorAddress);

const GAS_BUDGET = 10_000_000; // 0.01 SUI
// docs::/#server-setup

// docs::#sponsor-endpoint
app.post('/sponsor', async (req, res) => {
	try {
		// 1. Deserialize the client's transaction kind bytes.
		// The client sends kind-only bytes (built with onlyTransactionKind: true),
		// so use Transaction.fromKind() to reconstruct them.
		const { txBytes, sender } = req.body;
		const tx = Transaction.fromKind(fromBase64(txBytes));
		tx.setSender(sender);

		// 2. Acquire a gas coin from the pool
		const gasCoin = pool.acquire();
		if (!gasCoin) {
			res.status(503).json({ error: 'No gas coins available' });
			return;
		}

		try {
			// 3. Set gas payment from the sponsor
			tx.setGasOwner(sponsorAddress);
			tx.setGasBudget(GAS_BUDGET);
			tx.setGasPayment([{
				objectId: gasCoin.objectId,
				version: gasCoin.version,
				digest: gasCoin.digest,
			}]);

			// 4. Build the final transaction bytes
			const bytes = await tx.build({ client });

			// 5. Sign with the sponsor key
			const sponsorSig = await sponsor.signTransaction(bytes);

			// 6. Return the signed bytes, sponsor signature, and coin ID.
			//    The client must call POST /sponsor/confirm after submission
			//    so the pool can update the coin version and release it.
			res.json({
				txBytes: toBase64(bytes),
				sponsorSignature: sponsorSig.signature,
				gasCoinId: gasCoin.objectId,
			});
		} catch (error) {
			pool.release(gasCoin.objectId);
			throw error;
		}
	} catch (error) {
		res.status(400).json({ error: (error as Error).message });
	}
});
// docs::/#sponsor-endpoint

// docs::#confirm-endpoint
// Client calls this after submitting the dual-signed transaction.
// Without this callback, sponsored coins stay reserved and the pool drains.
app.post('/sponsor/confirm', async (req, res) => {
	const { gasCoinId, digest } = req.body;

	try {
		// Wait for the transaction to finalize, then re-fetch the gas coin
		// to get its updated version. Gas is always charged (even on
		// failed transactions), so the coin version changes in both cases.
		await client.waitForTransaction({ digest });

		const coinObj = await client.getObject({ id: gasCoinId });
		if (coinObj.data) {
			pool.release(gasCoinId, coinObj.data.version, coinObj.data.digest);
		}
		res.json({ ok: true });
	} catch {
		// RPC error: try a direct object fetch as fallback.
		try {
			const coinObj = await client.getObject({ id: gasCoinId });
			if (coinObj.data) {
				pool.release(gasCoinId, coinObj.data.version, coinObj.data.digest);
			}
		} catch {
			// Object fetch also failed. Remove the coin from the pool.
			// A background replenishment job replaces it.
			pool.discard(gasCoinId);
		}
		res.json({ ok: true });
	}
});
// docs::/#confirm-endpoint

// docs::#listen
app.listen(3001, () => console.log('Gas station running on :3001'));
// docs::/#listen
