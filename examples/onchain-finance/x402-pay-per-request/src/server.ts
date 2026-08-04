// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createServer } from "node:http";
import express from "express";
import { SuiGrpcClient } from "@mysten/sui/grpc";
import { TransactionDataBuilder } from "@mysten/sui/transactions";
import {
    fromBase64,
    isValidSuiAddress,
    normalizeStructTag,
    normalizeSuiAddress,
} from "@mysten/sui/utils";
import { verifyTransactionSignature } from "@mysten/sui/verify";

import { PaymentReservations, SettledPaymentStore } from "./replay-store.js";
import type { PaymentReservation } from "./replay-store.js";
import {
    EXACT_SCHEME,
    PAYMENT_REQUIRED_HEADER,
    PAYMENT_RESPONSE_HEADER,
    PAYMENT_SIGNATURE_HEADER,
    X402_VERSION,
    decodeHeaderValue,
    encodeHeaderValue,
    parsePaymentPayload,
} from "./x402.js";
import type {
    PaymentPayload,
    PaymentRequired,
    PaymentRequirements,
    SettlementResponse,
    X402ErrorReason,
} from "./x402.js";

// docs::#config
const NETWORK = "mainnet";
/** CAIP-2 network identifier, as required by x402 v2. */
const X402_NETWORK = `sui:${NETWORK}`;
const AMOUNT_MIST = 1_000_000n; // 0.001 SUI
const ASSET = normalizeStructTag("0x2::sui::SUI");
const MAX_TIMEOUT_SECONDS = 60;

/**
 * Read the recipient from the environment and fail fast. `normalizeSuiAddress`
 * pads any string to 32 bytes without validating it, so an unedited placeholder
 * would otherwise become a syntactically valid address that nobody controls.
 */
function requireAddress(value: string | undefined): string {
    if (!value || !isValidSuiAddress(normalizeSuiAddress(value))) {
        throw new Error("Set X402_PAY_TO to the address that receives payments");
    }
    return normalizeSuiAddress(value);
}

const PAY_TO = requireAddress(process.env.X402_PAY_TO);

const client = new SuiGrpcClient({
    baseUrl: "https://fullnode.mainnet.sui.io:443",
    network: NETWORK,
});

const settledPayments = new SettledPaymentStore(
    process.env.X402_SETTLED_PAYMENTS_PATH ?? "./x402-settled-payments.jsonl",
);
const reservations = new PaymentReservations(settledPayments);
// docs::/#config

// docs::#payment-required
/** The payment methods this server accepts, as advertised in the 402 response. */
function paymentRequirements(): PaymentRequirements {
    return {
        scheme: EXACT_SCHEME,
        network: X402_NETWORK,
        amount: AMOUNT_MIST.toString(),
        asset: ASSET,
        payTo: PAY_TO,
        maxTimeoutSeconds: MAX_TIMEOUT_SECONDS,
    };
}

function paymentRequired(req: express.Request, error: string): PaymentRequired {
    return {
        x402Version: X402_VERSION,
        error,
        resource: {
            url: `${req.protocol}://${req.get("host")}${req.originalUrl}`,
            description: "Protected resource",
            mimeType: "application/json",
        },
        accepts: [paymentRequirements()],
    };
}

/**
 * Rejects a payload whose `accepted` block does not match what this server
 * advertised. The signed transaction is verified separately: matching metadata
 * proves nothing on its own.
 */
function matchRequirements(accepted: PaymentRequirements): X402ErrorReason | null {
    if (accepted.scheme !== EXACT_SCHEME) return "invalid_scheme";
    if (accepted.network !== X402_NETWORK) return "invalid_network";
    if (normalizeStructTag(accepted.asset) !== ASSET) return "invalid_payment_requirements";
    if (normalizeSuiAddress(accepted.payTo) !== PAY_TO) return "invalid_payment_requirements";
    if (accepted.amount !== AMOUNT_MIST.toString()) return "invalid_payment_requirements";
    return null;
}
// docs::/#payment-required

interface VerifiedPayment {
    payload: PaymentPayload;
    transactionBytes: Uint8Array;
    digest: string;
    payer: string;
    reservation: PaymentReservation;
}

function settlementFailure(errorReason: X402ErrorReason, payer?: string): SettlementResponse {
    return {
        success: false,
        errorReason,
        transaction: "",
        network: X402_NETWORK,
        ...(payer ? { payer } : {}),
    };
}

function sendFailure(res: express.Response, status: number, body: SettlementResponse): void {
    res.setHeader(PAYMENT_RESPONSE_HEADER, encodeHeaderValue(body));
    res.status(status).json({ error: body.errorReason });
}

// docs::#verify-payment
/**
 * Verifies the payment authorization without moving funds, following the
 * verification steps of the `exact` scheme on Sui: check the network, check the
 * signature over the transaction, simulate to confirm the transaction would
 * succeed and has not already executed, and confirm the recipient receives
 * exactly the required amount of the required asset.
 */
const verifyPayment: express.RequestHandler = async (req, res, next) => {
    const header = req.get(PAYMENT_SIGNATURE_HEADER);
    if (!header) {
        res.setHeader(
            PAYMENT_REQUIRED_HEADER,
            encodeHeaderValue(paymentRequired(req, "PAYMENT-SIGNATURE header is required")),
        );
        res.status(402).json({ error: "payment required" });
        return;
    }

    let payload: PaymentPayload;
    let transactionBytes: Uint8Array;
    try {
        payload = parsePaymentPayload(decodeHeaderValue(header));
        transactionBytes = fromBase64(payload.payload.transaction);
    } catch {
        sendFailure(res, 400, settlementFailure("invalid_payload"));
        return;
    }

    const mismatch = matchRequirements(payload.accepted);
    if (mismatch) {
        sendFailure(res, 402, settlementFailure(mismatch));
        return;
    }

    // Reserve the digest synchronously, before the first await, so two
    // concurrent requests replaying one payload cannot both be served.
    let digest: string;
    let reservation: PaymentReservation;
    try {
        digest = TransactionDataBuilder.getDigestFromBytes(transactionBytes);
        reservation = reservations.reserve(digest);
    } catch {
        sendFailure(res, 402, settlementFailure("invalid_transaction_state"));
        return;
    }

    let payer: string;
    try {
        const publicKey = await verifyTransactionSignature(
            transactionBytes,
            payload.payload.signature,
        );
        payer = publicKey.toSuiAddress();
    } catch {
        reservation.rollback();
        sendFailure(res, 402, settlementFailure("invalid_payload"));
        return;
    }

    try {
        const simulation = await client.core.simulateTransaction({
            transaction: transactionBytes,
            include: { balanceChanges: true },
        });
        if (simulation.$kind !== "Transaction") {
            reservation.rollback();
            sendFailure(res, 402, settlementFailure("invalid_transaction_state", payer));
            return;
        }

        // The `exact` scheme requires the recipient to receive the exact amount,
        // not merely at least the amount.
        const paid = simulation.Transaction.balanceChanges.some(
            (change) =>
                normalizeSuiAddress(change.address) === PAY_TO &&
                normalizeStructTag(change.coinType) === ASSET &&
                BigInt(change.amount) === AMOUNT_MIST,
        );
        if (!paid) {
            reservation.rollback();
            sendFailure(res, 402, settlementFailure("insufficient_funds", payer));
            return;
        }
    } catch {
        reservation.rollback();
        sendFailure(res, 402, settlementFailure("unexpected_verify_error", payer));
        return;
    }

    const verified: VerifiedPayment = { payload, transactionBytes, digest, payer, reservation };
    res.locals.payment = verified;
    next();
};
// docs::/#verify-payment

// docs::#settle-payment
/**
 * Broadcasts the client's signed transaction. Settlement runs after the server
 * produces the response body, so a client is never charged for a request that
 * failed to do the work.
 */
async function settlePayment(res: express.Response): Promise<SettlementResponse> {
    const payment = res.locals.payment as VerifiedPayment;

    try {
        const result = await client.core.executeTransaction({
            transaction: payment.transactionBytes,
            signatures: [payment.payload.payload.signature],
        });
        if (result.$kind !== "Transaction") {
            payment.reservation.rollback();
            return settlementFailure("invalid_transaction_state", payment.payer);
        }

        payment.reservation.commit(payment.payer);
        return {
            success: true,
            transaction: result.Transaction.digest,
            network: X402_NETWORK,
            payer: payment.payer,
            amount: AMOUNT_MIST.toString(),
        };
    } catch {
        // The transaction may or may not have landed. Hold the reservation so a
        // replay of the same payload cannot be served, and reconcile the digest
        // out of band.
        return settlementFailure("unexpected_settle_error", payment.payer);
    }
}
// docs::/#settle-payment

// docs::#app
const app = express();
app.disable("x-powered-by");

app.get("/api/resource", verifyPayment, async (_req, res) => {
    // Do the work first, then settle.
    const body = { data: "Protected resource content" };

    const settlement = await settlePayment(res);
    res.setHeader(PAYMENT_RESPONSE_HEADER, encodeHeaderValue(settlement));
    if (!settlement.success) {
        res.status(402).json({ error: settlement.errorReason });
        return;
    }

    res.json(body);
});

// A signed Sui transaction does not always fit in Node's default 16 KiB header
// budget once other headers are present, so raise the limit for this listener.
createServer({ maxHeaderSize: 64 * 1024 }, app).listen(3000);
// docs::/#app

export { matchRequirements, paymentRequirements, settlePayment, verifyPayment };
export type { VerifiedPayment };
