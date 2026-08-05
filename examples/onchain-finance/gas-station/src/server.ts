// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import crypto from 'node:crypto';
import express from 'express';
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';
import {
	fromBase64,
	isValidSuiAddress,
	normalizeStructTag,
	normalizeSuiAddress,
	normalizeSuiObjectId,
	toBase64,
} from '@mysten/sui/utils';

import { GasCoinPool } from './pool.js';

// docs::#server-setup
const app = express();
app.use(express.json({ limit: '32kb' }));

const client = new SuiGrpcClient({
	network: 'testnet',
	baseUrl: 'https://fullnode.testnet.sui.io:443',
});
const sponsor = Ed25519Keypair.fromSecretKey(process.env.SPONSOR_SECRET_KEY!);
const sponsorAddress = sponsor.toSuiAddress();

const GAS_BUDGET = 10_000_000;
const MAX_COMMANDS = 4;
const MAX_INPUTS = 16;
const MAX_TYPE_ARGUMENTS = 2;
const RATE_LIMIT = 10;
const RATE_WINDOW_MS = 60_000;
const RECONCILE_INTERVAL_MS = 30_000;

// Reserve headroom above the budget so a coin is never handed out when it
// cannot actually pay for the transaction it is about to sponsor.
const MIN_COIN_BALANCE = BigInt(GAS_BUDGET) * 2n;

const pool = new GasCoinPool(MIN_COIN_BALANCE);
await pool.initialize(client, sponsorAddress);

function normalizeMoveTarget(target: string): string {
	const parts = target.split('::');
	if (parts.length !== 3 || parts.some((part) => part.length === 0)) {
		throw new Error(`Malformed Move target: ${target}`);
	}
	const [packageId, module, functionName] = parts;
	return `${normalizeSuiObjectId(packageId)}::${module}::${functionName}`;
}

function parseList(value: string | undefined): string[] {
	return (value ?? '')
		.split(',')
		.map((entry) => entry.trim())
		.filter(Boolean);
}

// Normalize both sides of the comparison. Package IDs appear in both short and
// 32-byte padded form, and an unnormalized Set silently rejects valid calls.
const allowedMoveCalls = new Set(parseList(process.env.ALLOWED_MOVE_CALLS).map(normalizeMoveTarget));
const allowedTypeArguments = new Set(
	parseList(process.env.ALLOWED_TYPE_ARGUMENTS).map(normalizeStructTag),
);

// Fail to start rather than start with an empty allowlist.
if (allowedMoveCalls.size === 0) {
	throw new Error('ALLOWED_MOVE_CALLS must list at least one Move target');
}

function hashSecret(value: string): string {
	return crypto.createHash('sha256').update(value).digest('hex');
}

// Per-client credentials, so one caller's rate limit cannot starve the others.
// Format: SPONSOR_API_KEYS="clientA:secretA,clientB:secretB"
const clientIdsByKeyHash = new Map(
	parseList(process.env.SPONSOR_API_KEYS).map((entry) => {
		const separator = entry.indexOf(':');
		if (separator <= 0 || separator === entry.length - 1) {
			throw new Error('SPONSOR_API_KEYS entries must be clientId:secret');
		}
		return [hashSecret(entry.slice(separator + 1)), entry.slice(0, separator)] as const;
	}),
);
if (clientIdsByKeyHash.size === 0) {
	throw new Error('SPONSOR_API_KEYS must define at least one credential');
}

interface PendingSponsorship {
	expectedDigest: string;
	coinVersionAtSponsorship: string;
	expiresAfterEpoch: bigint;
}

class ValidationError extends Error {
	constructor(message: string) {
		super(message);
		this.name = 'ValidationError';
	}
}

const pendingSponsorships = new Map<string, PendingSponsorship>();
const claimedSponsorships = new Set<string>();
const requestCounts = new Map<string, { count: number; resetAt: number }>();
// docs::/#server-setup

// Hashing both sides gives a fixed-length comparison and avoids leaking the
// key length that a direct timingSafeEqual on raw input would expose.
function authenticate(req: express.Request): string | null {
	const supplied = req.header('authorization')?.replace(/^Bearer /, '');
	if (!supplied) return null;
	return clientIdsByKeyHash.get(hashSecret(supplied)) ?? null;
}

function canConsume(key: string, now: number): boolean {
	const current = requestCounts.get(key);
	return !current || now >= current.resetAt || current.count < RATE_LIMIT;
}

function consume(key: string, now: number): void {
	const current = requestCounts.get(key);
	if (!current || now >= current.resetAt) {
		requestCounts.set(key, { count: 1, resetAt: now + RATE_WINDOW_MS });
		return;
	}
	current.count += 1;
}

// Check every bucket before consuming any, so a request rejected on the sender
// bucket does not still burn the client's quota.
function enforceRateLimit(clientId: string, sender: string): boolean {
	const now = Date.now();
	for (const [key, value] of requestCounts) {
		if (now >= value.resetAt) requestCounts.delete(key);
	}
	const keys = [`client:${clientId}`, `sender:${sender}`];
	if (!keys.every((key) => canConsume(key, now))) return false;
	for (const key of keys) consume(key, now);
	return true;
}

function validateTransactionKind(tx: Transaction): void {
	const data = tx.getData();
	if (data.commands.length === 0 || data.commands.length > MAX_COMMANDS) {
		throw new ValidationError('Transaction has an invalid command count');
	}
	if (data.inputs.length > MAX_INPUTS) throw new ValidationError('Transaction has too many inputs');
	if (data.inputs.some((input) => input.$kind === 'Object' || input.$kind === 'UnresolvedObject')) {
		throw new ValidationError('This example policy does not allow object inputs');
	}

	for (const command of data.commands) {
		// This example sponsors only explicitly approved Move calls. In
		// particular, TransferObjects and SplitCoins are rejected so a client
		// cannot transfer or split the sponsor's GasCoin.
		if (command.$kind !== 'MoveCall') throw new ValidationError('Transaction command is not allowed');
		const moveCall = command.MoveCall;
		const target = normalizeMoveTarget(
			`${moveCall.package}::${moveCall.module}::${moveCall.function}`,
		);
		if (!allowedMoveCalls.has(target)) throw new ValidationError(`Move call is not allowed: ${target}`);
		if (moveCall.arguments.some((argument) => argument.$kind === 'GasCoin')) {
			throw new ValidationError('Move calls cannot use the sponsor gas coin');
		}
		// An allowlisted generic function still reaches arbitrary code through
		// an attacker-chosen type argument, so bound and allowlist them too.
		if (moveCall.typeArguments.length > MAX_TYPE_ARGUMENTS) {
			throw new ValidationError('Move call has too many type arguments');
		}
		for (const typeArgument of moveCall.typeArguments) {
			if (!allowedTypeArguments.has(normalizeStructTag(typeArgument))) {
				throw new ValidationError(`Type argument is not allowed: ${typeArgument}`);
			}
		}
	}
}

async function getCurrentEpoch(): Promise<bigint> {
	const { epoch } = await client.getCurrentEpoch();
	return BigInt(epoch.epochId);
}

async function refreshGasCoin(gasCoinId: string): Promise<void> {
	const { object } = await client.getObject({
		objectId: gasCoinId,
		include: { content: true },
	});
	if (!object || normalizeSuiAddress(object.owner?.AddressOwner ?? '') !== sponsorAddress) {
		pool.discard(gasCoinId);
		return;
	}
	pool.release(gasCoinId, object.version, object.digest, BigInt(object.balance ?? 0n));
}

// docs::#sponsor-endpoint
app.post('/sponsor', async (req, res) => {
	let gasCoinId: string | undefined;
	let signed = false;
	try {
		const clientId = authenticate(req);
		if (!clientId) {
			res.status(401).json({ error: 'Authentication required' });
			return;
		}

		const { txBytes, sender } = req.body as { txBytes?: unknown; sender?: unknown };
		if (
			typeof txBytes !== 'string' ||
			typeof sender !== 'string' ||
			!isValidSuiAddress(sender)
		) {
			throw new ValidationError('Invalid transaction bytes or sender');
		}
		const normalizedSender = normalizeSuiAddress(sender);
		if (!enforceRateLimit(clientId, normalizedSender)) {
			res.status(429).json({ error: 'Sponsorship rate limit exceeded' });
			return;
		}

		const tx = Transaction.fromKind(fromBase64(txBytes));
		validateTransactionKind(tx);
		tx.setSender(normalizedSender);

		const gasCoin = pool.acquire();
		if (!gasCoin) {
			res.status(503).json({ error: 'No gas coins available' });
			return;
		}
		gasCoinId = gasCoin.objectId;

		const expirationEpoch = await getCurrentEpoch();
		tx.setExpiration({ Epoch: Number(expirationEpoch) });
		tx.setGasOwner(sponsorAddress);
		tx.setGasBudget(GAS_BUDGET);
		tx.setGasPayment([
			{ objectId: gasCoin.objectId, version: gasCoin.version, digest: gasCoin.digest },
		]);

		const bytes = await tx.build({ client });
		const simulation = await client.simulateTransaction({ transaction: bytes });
		if (!simulation.transaction.effects.status.success) {
			throw new ValidationError(
				`Transaction simulation failed: ${
					simulation.transaction.effects.status.error ?? 'unknown error'
				}`,
			);
		}

		const builtTx = Transaction.from(bytes);
		const expectedDigest = builtTx.getDigest();
		const sponsorSig = await sponsor.signTransaction(bytes);
		// Past this point a valid signature exists for this coin version. The
		// error path must not release the coin, or a later request would sign a
		// second transaction against the same version and one of them dies.
		signed = true;
		pendingSponsorships.set(gasCoin.objectId, {
			expectedDigest,
			coinVersionAtSponsorship: gasCoin.version,
			expiresAfterEpoch: expirationEpoch,
		});

		res.json({
			txBytes: toBase64(bytes),
			sponsorSignature: sponsorSig.signature,
			sponsorAddress,
			gasCoinId: gasCoin.objectId,
			digest: expectedDigest,
		});
	} catch (error) {
		if (gasCoinId && !signed) pool.release(gasCoinId);
		if (error instanceof ValidationError) {
			res.status(400).json({ error: error.message });
		} else {
			res.status(500).json({ error: 'Internal server error' });
		}
	}
});
// docs::/#sponsor-endpoint

// docs::#confirm-endpoint
app.post('/sponsor/confirm', async (req, res) => {
	if (!authenticate(req)) {
		res.status(401).json({ error: 'Authentication required' });
		return;
	}
	const { gasCoinId, digest } = req.body as { gasCoinId?: unknown; digest?: unknown };
	if (typeof gasCoinId !== 'string' || typeof digest !== 'string') {
		res.status(400).json({ error: 'Invalid confirmation' });
		return;
	}

	const pending = pendingSponsorships.get(gasCoinId);
	if (!pending || digest !== pending.expectedDigest || claimedSponsorships.has(gasCoinId)) {
		res.status(400).json({ error: 'Confirmation does not match an available sponsorship' });
		return;
	}

	// Claim before awaiting so confirmation and reconciliation cannot release
	// the same coin twice or delete a newer reservation for the same coin ID.
	claimedSponsorships.add(gasCoinId);
	try {
		await client.waitForTransaction({ digest: pending.expectedDigest });
		if (pendingSponsorships.get(gasCoinId) !== pending) throw new Error('Reservation changed');
		pendingSponsorships.delete(gasCoinId);
		await refreshGasCoin(gasCoinId);
		res.json({ ok: true });
	} catch {
		if (!pendingSponsorships.has(gasCoinId)) pendingSponsorships.set(gasCoinId, pending);
		// Keep the coin reserved. The reconciler releases it after observing
		// finality, or after its epoch expiration makes late submission invalid.
		res.status(202).json({ ok: false, pending: true });
	} finally {
		claimedSponsorships.delete(gasCoinId);
	}
});
// docs::/#confirm-endpoint

// docs::#reservation-reconciliation
async function reconcileReservations(): Promise<void> {
	const currentEpoch = await getCurrentEpoch();

	for (const [gasCoinId, pending] of pendingSponsorships) {
		if (claimedSponsorships.has(gasCoinId)) continue;
		claimedSponsorships.add(gasCoinId);
		try {
			let releasable = false;
			try {
				await client.getTransaction({ digest: pending.expectedDigest });
				releasable = true;
			} catch {
				releasable = currentEpoch > pending.expiresAfterEpoch;
			}
			if (releasable && pendingSponsorships.get(gasCoinId) === pending) {
				pendingSponsorships.delete(gasCoinId);
				try {
					await refreshGasCoin(gasCoinId);
				} catch {
					pendingSponsorships.set(gasCoinId, pending);
				}
			}
		} finally {
			claimedSponsorships.delete(gasCoinId);
		}
	}
}

setInterval(() => void reconcileReservations(), RECONCILE_INTERVAL_MS).unref();
// docs::/#reservation-reconciliation

// docs::#listen
app.listen(3001, () => console.log('Gas station running on :3001'));
// docs::/#listen
