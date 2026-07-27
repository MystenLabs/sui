// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import express from 'express';
import { Transaction } from '@mysten/sui/transactions';
import { SuiClient } from '@mysten/sui/client';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { toBase64, fromBase64 } from '@mysten/sui/utils';
import { TransactionDataBuilder } from '@mysten/sui/transactions';

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

// Track pending sponsorships: gasCoinId → expected digest and coin version.
// The confirm endpoint requires the exact digest the server computed at
// sponsor time. The coin is only released after verifying the transaction
// onchain or confirming the coin version advanced.
interface PendingSponsorship {
	expectedDigest: string;
	coinVersionAtSponsorship: string;
}
const pendingSponsorships = new Map<string, PendingSponsorship>();
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

			// 4. Build the final transaction bytes and compute the digest
			const bytes = await tx.build({ client });
			const builtB64 = toBase64(bytes);
			const expectedDigest = TransactionDataBuilder.getDigestFromBytes(bytes);

			// 5. Sign with the sponsor key
			const sponsorSig = await sponsor.signTransaction(bytes);

			// 6. Record the expected digest so /confirm can verify
			pendingSponsorships.set(gasCoin.objectId, {
				expectedDigest,
				coinVersionAtSponsorship: gasCoin.version,
			});

			// 7. Return the signed bytes, sponsor signature, and coin ID.
			res.json({
				txBytes: builtB64,
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
// The server only releases the gas coin after verifying the provided
// digest matches the transaction it sponsored and the coin version
// has advanced onchain.
app.post('/sponsor/confirm', async (req, res) => {
	const { gasCoinId, digest } = req.body;

	// 1. Verify this gas coin has a pending sponsorship
	const pending = pendingSponsorships.get(gasCoinId);
	if (!pending) {
		res.status(400).json({ error: 'No pending sponsorship for this gas coin' });
		return;
	}

	// 2. Reject if the digest does not match what the server signed
	if (digest !== pending.expectedDigest) {
		res.status(400).json({ error: 'Digest does not match the sponsored transaction' });
		return;
	}

	try {
		// 3. Wait for the exact sponsored transaction to finalize
		await client.waitForTransaction({ digest: pending.expectedDigest });

		// 4. Re-fetch the gas coin to get its updated version
		const coinObj = await client.getObject({ id: gasCoinId });
		if (coinObj.data) {
			pool.release(gasCoinId, coinObj.data.version, coinObj.data.digest);
		}
		pendingSponsorships.delete(gasCoinId);
		res.json({ ok: true });
	} catch {
		// RPC error or timeout. Only release the coin if we can confirm
		// its version advanced (proving the sponsored tx executed).
		try {
			const coinObj = await client.getObject({ id: gasCoinId });
			if (coinObj.data && coinObj.data.version !== pending.coinVersionAtSponsorship) {
				// Coin version advanced: the sponsored tx (or some tx) consumed it.
				pool.release(gasCoinId, coinObj.data.version, coinObj.data.digest);
				pendingSponsorships.delete(gasCoinId);
			}
			// If version unchanged, keep the coin reserved. The client
			// can retry /confirm or the sponsorship times out via a
			// background cleanup job (not shown).
		} catch {
			// Cannot reach RPC at all. Keep the coin reserved rather
			// than releasing it unsafely. A background job should
			// periodically reconcile stale reservations.
		}
		res.json({ ok: true });
	}
});
// docs::/#confirm-endpoint

// docs::#listen
app.listen(3001, () => console.log('Gas station running on :3001'));
// docs::/#listen
