// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import crypto from "node:crypto";
import express from "express";
import { SuiClient } from "@mysten/sui/client";
import { Ed25519Keypair } from "@mysten/sui/keypairs/ed25519";
import { Transaction, TransactionDataBuilder } from "@mysten/sui/transactions";
import { fromBase64, isValidSuiAddress, normalizeSuiAddress, toBase64 } from "@mysten/sui/utils";

import { GasCoinPool } from "./pool.js";

// docs::#server-setup
const app = express();
app.use(express.json({ limit: "32kb" }));

const client = new SuiClient({ url: "https://fullnode.testnet.sui.io:443" });
const sponsor = Ed25519Keypair.fromSecretKey(process.env.SPONSOR_SECRET_KEY!);
const sponsorAddress = sponsor.toSuiAddress();
const sponsorApiKey = process.env.SPONSOR_API_KEY!;

const pool = new GasCoinPool();
await pool.initialize(client, sponsorAddress);

const GAS_BUDGET = 10_000_000;
const MAX_COMMANDS = 4;
const MAX_INPUTS = 16;
const RATE_LIMIT = 10;
const RATE_WINDOW_MS = 60_000;
const RECONCILE_INTERVAL_MS = 30_000;
const allowedMoveCalls = new Set(
    (process.env.ALLOWED_MOVE_CALLS ?? "0xPACKAGE::module::function")
        .split(",")
        .map((value) => value.trim()),
);

interface PendingSponsorship {
    expectedDigest: string;
    coinVersionAtSponsorship: string;
    expiresAfterEpoch: bigint;
}

const pendingSponsorships = new Map<string, PendingSponsorship>();
const claimedSponsorships = new Set<string>();
const requestCounts = new Map<string, { count: number; resetAt: number }>();
// docs::/#server-setup

function authenticate(req: express.Request): boolean {
    const supplied = req.header("authorization")?.replace(/^Bearer /, "");
    if (!supplied || !sponsorApiKey) return false;
    const actual = Buffer.from(supplied);
    const expected = Buffer.from(sponsorApiKey);
    return actual.length === expected.length && crypto.timingSafeEqual(actual, expected);
}

function consumeRateLimit(key: string, now: number): boolean {
    const current = requestCounts.get(key);
    if (!current || now >= current.resetAt) {
        requestCounts.set(key, { count: 1, resetAt: now + RATE_WINDOW_MS });
        return true;
    }
    if (current.count >= RATE_LIMIT) return false;
    current.count += 1;
    return true;
}

function enforceRateLimit(sender: string): boolean {
    const now = Date.now();
    for (const [key, value] of requestCounts) {
        if (now >= value.resetAt) requestCounts.delete(key);
    }
    // Limit the authenticated credential as well as the claimed sender so a
    // caller cannot bypass the limit by rotating sender addresses.
    return consumeRateLimit("credential", now) && consumeRateLimit(`sender:${sender}`, now);
}

function validateTransactionKind(tx: Transaction): void {
    const data = tx.getData();
    if (data.commands.length === 0 || data.commands.length > MAX_COMMANDS) {
        throw new Error("Transaction has an invalid command count");
    }
    if (data.inputs.length > MAX_INPUTS) throw new Error("Transaction has too many inputs");
    if (
        data.inputs.some((input) => input.$kind === "Object" || input.$kind === "UnresolvedObject")
    ) {
        throw new Error("This example policy does not allow object inputs");
    }

    for (const command of data.commands) {
        // This example sponsors only explicitly approved Move calls. In
        // particular, TransferObjects and SplitCoins are rejected so a client
        // cannot transfer or split the sponsor's GasCoin.
        if (command.$kind !== "MoveCall") throw new Error("Transaction command is not allowed");
        const moveCall = command.MoveCall;
        const target = `${moveCall.package}::${moveCall.module}::${moveCall.function}`;
        if (!allowedMoveCalls.has(target)) throw new Error(`Move call is not allowed: ${target}`);
        if (moveCall.arguments.some((argument) => argument.$kind === "GasCoin")) {
            throw new Error("Move calls cannot use the sponsor gas coin");
        }
    }
}

async function refreshGasCoin(gasCoinId: string): Promise<void> {
    const coinObj = await client.getObject({ id: gasCoinId, options: { showOwner: true } });
    const owner = coinObj.data?.owner;
    if (
        !coinObj.data ||
        owner === null ||
        typeof owner !== "object" ||
        !("AddressOwner" in owner) ||
        normalizeSuiAddress(owner.AddressOwner) !== sponsorAddress
    ) {
        pool.discard(gasCoinId);
        return;
    }
    pool.release(gasCoinId, coinObj.data.version, coinObj.data.digest);
}

// docs::#sponsor-endpoint
app.post("/sponsor", async (req, res) => {
    let gasCoinId: string | undefined;
    try {
        if (!authenticate(req)) {
            res.status(401).json({ error: "Authentication required" });
            return;
        }

        const { txBytes, sender } = req.body as { txBytes?: unknown; sender?: unknown };
        if (
            typeof txBytes !== "string" ||
            typeof sender !== "string" ||
            !isValidSuiAddress(sender)
        ) {
            throw new Error("Invalid transaction bytes or sender");
        }
        const normalizedSender = normalizeSuiAddress(sender);
        if (!enforceRateLimit(normalizedSender)) {
            res.status(429).json({ error: "Sponsorship rate limit exceeded" });
            return;
        }

        const tx = Transaction.fromKind(fromBase64(txBytes));
        validateTransactionKind(tx);
        tx.setSender(normalizedSender);

        const gasCoin = pool.acquire();
        if (!gasCoin) {
            res.status(503).json({ error: "No gas coins available" });
            return;
        }
        gasCoinId = gasCoin.objectId;

        const { epoch } = await client.getLatestSuiSystemState();
        const expirationEpoch = BigInt(epoch);
        tx.setExpiration({ Epoch: epoch });
        tx.setGasOwner(sponsorAddress);
        tx.setGasBudget(GAS_BUDGET);
        tx.setGasPayment([
            { objectId: gasCoin.objectId, version: gasCoin.version, digest: gasCoin.digest },
        ]);

        const bytes = await tx.build({ client });
        const dryRun = await client.dryRunTransactionBlock({ transactionBlock: bytes });
        if (dryRun.effects.status.status !== "success") {
            throw new Error(
                `Transaction dry run failed: ${dryRun.effects.status.error ?? "unknown error"}`,
            );
        }

        const expectedDigest = TransactionDataBuilder.getDigestFromBytes(bytes);
        const sponsorSig = await sponsor.signTransaction(bytes);
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
        if (gasCoinId) pool.release(gasCoinId);
        res.status(400).json({ error: (error as Error).message });
    }
});
// docs::/#sponsor-endpoint

// docs::#confirm-endpoint
app.post("/sponsor/confirm", async (req, res) => {
    if (!authenticate(req)) {
        res.status(401).json({ error: "Authentication required" });
        return;
    }
    const { gasCoinId, digest } = req.body as { gasCoinId?: unknown; digest?: unknown };
    if (typeof gasCoinId !== "string" || typeof digest !== "string") {
        res.status(400).json({ error: "Invalid confirmation" });
        return;
    }

    const pending = pendingSponsorships.get(gasCoinId);
    if (!pending || digest !== pending.expectedDigest || claimedSponsorships.has(gasCoinId)) {
        res.status(400).json({ error: "Confirmation does not match an available sponsorship" });
        return;
    }

    // Claim before awaiting so confirmation and reconciliation cannot release
    // the same coin twice or delete a newer reservation for the same coin ID.
    claimedSponsorships.add(gasCoinId);
    try {
        await client.waitForTransaction({ digest: pending.expectedDigest });
        if (pendingSponsorships.get(gasCoinId) !== pending) throw new Error("Reservation changed");
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
    const { epoch } = await client.getLatestSuiSystemState();
    const currentEpoch = BigInt(epoch);

    for (const [gasCoinId, pending] of pendingSponsorships) {
        if (claimedSponsorships.has(gasCoinId)) continue;
        claimedSponsorships.add(gasCoinId);
        try {
            let releasable = false;
            try {
                await client.getTransactionBlock({ digest: pending.expectedDigest });
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
app.listen(3001, () => console.log("Gas station running on :3001"));
// docs::/#listen
