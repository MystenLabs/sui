// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiGrpcClient } from "@mysten/sui/grpc";
import { Ed25519Keypair } from "@mysten/sui/keypairs/ed25519";
import { Transaction } from "@mysten/sui/transactions";
import { normalizeStructTag, normalizeSuiAddress, toBase64 } from "@mysten/sui/utils";

import {
    EXACT_SCHEME,
    PAYMENT_REQUIRED_HEADER,
    PAYMENT_RESPONSE_HEADER,
    PAYMENT_SIGNATURE_HEADER,
    X402_VERSION,
    decodeHeaderValue,
    encodeHeaderValue,
    parsePaymentRequired,
    parseSettlementResponse,
} from "./x402.js";
import type {
    PaymentPayload,
    PaymentRequired,
    PaymentRequirements,
    SettlementResponse,
} from "./x402.js";

const SUI_COIN_TYPE = normalizeStructTag("0x2::sui::SUI");

const client = new SuiGrpcClient({
    baseUrl: "https://fullnode.mainnet.sui.io:443",
    network: "mainnet",
});
let cachedKeypair: Ed25519Keypair | undefined;

/** Loaded on first use so the module can be imported without a key present. */
function agentKeypair(): Ed25519Keypair {
    if (!cachedKeypair) {
        const secretKey = process.env.AGENT_SECRET_KEY;
        if (!secretKey) throw new Error("Set AGENT_SECRET_KEY to a suiprivkey-encoded key");
        cachedKeypair = Ed25519Keypair.fromSecretKey(secretKey);
    }
    return cachedKeypair;
}

// docs::#payment-policy
/** Limits the agent enforces locally before signing anything. */
interface PaymentPolicy {
    expectedOrigin: string;
    expectedPayTo: string;
    allowedNetworks: readonly string[];
    allowedAssets: readonly string[];
    maxAmount: bigint;
}

/**
 * Picks the first advertised payment method that satisfies the policy. A server
 * can advertise several; none of them is trusted until it passes these checks.
 */
function selectPaymentRequirements(
    required: PaymentRequired,
    policy: PaymentPolicy,
): PaymentRequirements {
    const allowedAssets = policy.allowedAssets.map(normalizeStructTag);
    const expectedPayTo = normalizeSuiAddress(policy.expectedPayTo);

    for (const candidate of required.accepts) {
        if (candidate.scheme !== EXACT_SCHEME) continue;
        if (!policy.allowedNetworks.includes(candidate.network)) continue;
        if (!allowedAssets.includes(normalizeStructTag(candidate.asset))) continue;
        if (normalizeSuiAddress(candidate.payTo) !== expectedPayTo) continue;

        const amount = BigInt(candidate.amount);
        if (amount <= 0n || amount > policy.maxAmount) continue;

        return candidate;
    }

    throw new Error("No advertised payment method satisfies the policy");
}
// docs::/#payment-policy

// docs::#build-payment
/**
 * Builds and signs the payment transaction. The client never broadcasts it: the
 * resource server settles it after serving the request, so a rejected request
 * costs nothing.
 */
async function signPayment(requirements: PaymentRequirements): Promise<PaymentPayload["payload"]> {
    const keypair = agentKeypair();
    const sender = keypair.toSuiAddress();
    const payTo = normalizeSuiAddress(requirements.payTo);
    const asset = normalizeStructTag(requirements.asset);
    const amount = BigInt(requirements.amount);

    const tx = new Transaction();
    tx.setSender(sender);

    if (asset === SUI_COIN_TYPE) {
        const [payment] = tx.splitCoins(tx.gas, [tx.pure.u64(amount)]);
        tx.transferObjects([payment], payTo);
    } else {
        const { objects: coins } = await client.core.listCoins({ owner: sender, coinType: asset });

        const selected: string[] = [];
        let total = 0n;
        for (const coin of coins) {
            selected.push(coin.objectId);
            total += BigInt(coin.balance);
            if (total >= amount) break;
        }
        const [primaryId, ...restIds] = selected;
        if (!primaryId || total < amount) {
            throw new Error(`Insufficient ${asset} balance for payment`);
        }

        // Merge only as many coins as the payment needs, so the transaction
        // stays small enough to fit in an HTTP header.
        const primary = tx.object(primaryId);
        if (restIds.length > 0) {
            tx.mergeCoins(
                primary,
                restIds.map((objectId) => tx.object(objectId)),
            );
        }
        const [payment] = tx.splitCoins(primary, [tx.pure.u64(amount)]);
        tx.transferObjects([payment], payTo);
    }

    // Built with a client, so the transaction carries the 2.0 default
    // expiration of the current epoch plus one.
    const bytes = await tx.build({ client });
    const { signature } = await keypair.signTransaction(bytes);

    return { signature, transaction: toBase64(bytes) };
}
// docs::/#build-payment

// docs::#fetch-with-payment
interface PaidResponse {
    response: Response;
    settlement?: SettlementResponse;
}

async function fetchWithPayment(url: string, policy: PaymentPolicy): Promise<PaidResponse> {
    const requestUrl = new URL(url);
    const expectedOrigin = new URL(policy.expectedOrigin).origin;
    if (requestUrl.origin !== expectedOrigin) {
        throw new Error("Request origin does not match policy");
    }

    // Do not follow redirects across the payment trust boundary.
    const response = await fetch(requestUrl, { redirect: "manual" });
    if (response.status !== 402) return { response };

    const header = response.headers.get(PAYMENT_REQUIRED_HEADER);
    if (!header) throw new Error("402 response is missing the PAYMENT-REQUIRED header");

    const required = parsePaymentRequired(decodeHeaderValue(header));
    if (new URL(required.resource.url).origin !== expectedOrigin) {
        throw new Error("Payment instructions name an unexpected resource origin");
    }

    const requirements = selectPaymentRequirements(required, policy);
    const payload: PaymentPayload = {
        x402Version: X402_VERSION,
        resource: required.resource,
        accepted: requirements,
        payload: await signPayment(requirements),
    };

    const paid = await fetch(requestUrl, {
        redirect: "manual",
        headers: { [PAYMENT_SIGNATURE_HEADER]: encodeHeaderValue(payload) },
    });

    const settlementHeader = paid.headers.get(PAYMENT_RESPONSE_HEADER);
    if (!settlementHeader) return { response: paid };

    return {
        response: paid,
        settlement: parseSettlementResponse(decodeHeaderValue(settlementHeader)),
    };
}
// docs::/#fetch-with-payment

export { fetchWithPayment, selectPaymentRequirements, signPayment };
export type { PaidResponse, PaymentPolicy };
