// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import crypto from "node:crypto";
import express from "express";
import { SuiClient } from "@mysten/sui/client";
import { normalizeStructTag, normalizeSuiAddress } from "@mysten/sui/utils";
import { verifyPersonalMessageSignature } from "@mysten/sui/verify";

import { ChallengeStore, DurableDigestStore } from "./replay-store.js";

// docs::#config
const PAYMENT_RECIPIENT = normalizeSuiAddress("0xYOUR_SERVER_ADDRESS");
const PRICE_MIST = 1_000_000n; // 0.001 SUI
const COIN_TYPE = normalizeStructTag("0x2::sui::SUI");
const CHALLENGE_TTL_MS = 5 * 60 * 1000;
const CHALLENGE_RATE_LIMIT = 30;
const CHALLENGE_RATE_WINDOW_MS = 60_000;

const client = new SuiClient({ url: "https://fullnode.mainnet.sui.io:443" });
// docs::/#config

// docs::#challenge-store
// Challenges expire quickly, but accepted digests must never expire: the
// payment itself does not contain the challenge, so deleting a used digest
// would make that historic payment reusable with a fresh challenge.
const usedDigests = new DurableDigestStore(
    process.env.X402_USED_DIGESTS_PATH ?? "./x402-used-digests.jsonl",
);
const challenges = new ChallengeStore(usedDigests);
const challengeRequestCounts = new Map<string, { count: number; resetAt: number }>();

function generateChallengeId(): string {
    return crypto.randomUUID();
}
// docs::/#challenge-store

function getHeader(req: express.Request, name: string): string | null {
    const value = req.headers[name];
    return typeof value === "string" && value.length > 0 ? value : null;
}

function allowChallengeRequest(ip: string, now = Date.now()): boolean {
    for (const [key, value] of challengeRequestCounts) {
        if (now >= value.resetAt) challengeRequestCounts.delete(key);
    }
    const current = challengeRequestCounts.get(ip);
    if (!current) {
        challengeRequestCounts.set(ip, { count: 1, resetAt: now + CHALLENGE_RATE_WINDOW_MS });
        return true;
    }
    if (current.count >= CHALLENGE_RATE_LIMIT) return false;
    current.count += 1;
    return true;
}

// docs::#payment-required
const paymentRequired: express.RequestHandler = (req, res, next) => {
    const digest = getHeader(req, "x-payment-digest");
    const challengeId = getHeader(req, "x-payment-challenge");
    const challengeSignature = getHeader(req, "x-payment-signature");

    if (!digest || !challengeId || !challengeSignature) {
        if (!allowChallengeRequest(req.ip ?? req.socket.remoteAddress ?? "unknown")) {
            res.status(429).json({ error: "Challenge rate limit exceeded" });
            return;
        }
        const id = generateChallengeId();
        try {
            challenges.issue(id, Date.now() + CHALLENGE_TTL_MS);
        } catch (error) {
            res.status(503).json({ error: (error as Error).message });
            return;
        }

        res.status(402).json({
            amount: PRICE_MIST.toString(),
            recipient: PAYMENT_RECIPIENT,
            coinType: COIN_TYPE,
            challenge: id,
            message:
                "Payment required. Sign the challenge with your keypair, pay the amount, then retry with X-Payment-Digest, X-Payment-Challenge, and X-Payment-Signature headers.",
        });
        return;
    }

    next();
};
// docs::/#payment-required

// docs::#verify-payment
const verifyPayment: express.RequestHandler = async (req, res, next) => {
    const digest = getHeader(req, "x-payment-digest");
    const challengeId = getHeader(req, "x-payment-challenge");
    const challengeSignature = getHeader(req, "x-payment-signature");
    if (!digest || !challengeId || !challengeSignature) {
        res.status(400).json({ error: "Missing payment proof headers" });
        return;
    }

    // Reserve both identifiers synchronously before the first await. A second
    // concurrent request cannot verify the same challenge or digest.
    let reservation;
    try {
        reservation = challenges.reserve(challengeId, digest);
    } catch (error) {
        res.status(400).json({ error: (error as Error).message });
        return;
    }

    try {
        const challengeBytes = new TextEncoder().encode(challengeId);
        const publicKey = await verifyPersonalMessageSignature(challengeBytes, challengeSignature);
        const signerAddress = publicKey.toSuiAddress();

        const txResponse = await client.getTransactionBlock({
            digest,
            options: { showBalanceChanges: true, showInput: true, showEffects: true },
        });
        if (txResponse.effects?.status.status !== "success") {
            throw new Error("Payment transaction failed");
        }
        if (txResponse.transaction?.data.sender !== signerAddress) {
            res.status(403).json({ error: "Transaction sender does not match challenge signer" });
            reservation.rollback();
            return;
        }

        const received = (txResponse.balanceChanges ?? []).some((change) => {
            const owner = change.owner;
            return (
                typeof owner === "object" &&
                "AddressOwner" in owner &&
                normalizeSuiAddress(owner.AddressOwner) === PAYMENT_RECIPIENT &&
                normalizeStructTag(change.coinType) === COIN_TYPE &&
                BigInt(change.amount) >= PRICE_MIST
            );
        });
        if (!received) {
            res.status(402).json({ error: "Payment not found or insufficient amount" });
            reservation.rollback();
            return;
        }

        reservation.commit();
        next();
    } catch {
        reservation.rollback();
        res.status(402).json({ error: "Could not verify payment" });
    }
};
// docs::/#verify-payment

// docs::#app
const app = express();

app.get("/api/resource", paymentRequired, verifyPayment, (_req, res) => {
    res.json({ data: "Protected resource content" });
});

app.listen(3000);
// docs::/#app
