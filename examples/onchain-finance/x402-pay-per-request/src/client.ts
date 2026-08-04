// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient } from "@mysten/sui/client";
import { Ed25519Keypair } from "@mysten/sui/keypairs/ed25519";
import { Transaction } from "@mysten/sui/transactions";
import { normalizeStructTag, normalizeSuiAddress } from "@mysten/sui/utils";

const client = new SuiClient({ url: "https://fullnode.mainnet.sui.io:443" });
const keypair = Ed25519Keypair.fromSecretKey(process.env.AGENT_SECRET_KEY!);

interface PaymentPolicy {
    expectedOrigin: string;
    expectedRecipient: string;
    allowedCoinTypes: readonly string[];
    maxAmountMist: bigint;
}

interface PaymentInstructions {
    amount: bigint;
    recipient: string;
    coinType: string;
    challenge: string;
}

function parsePaymentInstructions(value: unknown, policy: PaymentPolicy): PaymentInstructions {
    if (!value || typeof value !== "object") throw new Error("Invalid payment instructions");
    const record = value as Record<string, unknown>;
    if (
        typeof record.amount !== "string" ||
        !/^[0-9]+$/.test(record.amount) ||
        typeof record.recipient !== "string" ||
        typeof record.coinType !== "string" ||
        typeof record.challenge !== "string"
    ) {
        throw new Error("Invalid payment instructions");
    }

    if (!/^[0-9a-f-]{36}$/i.test(record.challenge) || record.challenge.length !== 36) {
        throw new Error("Invalid payment challenge");
    }

    const amount = BigInt(record.amount);
    const recipient = normalizeSuiAddress(record.recipient);
    const coinType = normalizeStructTag(record.coinType);
    if (recipient !== normalizeSuiAddress(policy.expectedRecipient)) {
        throw new Error("Payment recipient does not match policy");
    }
    if (!policy.allowedCoinTypes.map(normalizeStructTag).includes(coinType)) {
        throw new Error("Payment coin type is not allowed");
    }
    if (amount <= 0n || amount > policy.maxAmountMist) {
        throw new Error("Payment amount exceeds policy");
    }

    return { amount, recipient, coinType, challenge: record.challenge };
}

// docs::#fetch-with-payment
async function fetchWithPayment(url: string, policy: PaymentPolicy): Promise<Response> {
    const requestUrl = new URL(url);
    const expectedOrigin = new URL(policy.expectedOrigin).origin;
    if (requestUrl.origin !== expectedOrigin)
        throw new Error("Request origin does not match policy");

    // Do not follow redirects across the payment trust boundary.
    const response = await fetch(requestUrl, { redirect: "manual" });
    if (response.url && new URL(response.url).origin !== expectedOrigin) {
        throw new Error("Payment instructions came from an unexpected origin");
    }
    if (response.status !== 402) return response;

    const { amount, recipient, coinType, challenge } = parsePaymentInstructions(
        await response.json(),
        policy,
    );
    const challengeBytes = new TextEncoder().encode(challenge);
    const { signature: challengeSignature } = await keypair.signPersonalMessage(challengeBytes);

    const tx = new Transaction();
    tx.setSender(keypair.toSuiAddress());
    if (coinType === normalizeStructTag("0x2::sui::SUI")) {
        const [coin] = tx.splitCoins(tx.gas, [amount]);
        tx.transferObjects([coin], recipient);
    } else {
        const { data: coins } = await client.getCoins({
            owner: keypair.toSuiAddress(),
            coinType,
        });
        const paymentCoin = coins.find((coin) => BigInt(coin.balance) >= amount);
        if (!paymentCoin) throw new Error("No single coin can cover the payment");
        const [coin] = tx.splitCoins(tx.object(paymentCoin.coinObjectId), [amount]);
        tx.transferObjects([coin], recipient);
    }

    const result = await client.signAndExecuteTransaction({
        transaction: tx,
        signer: keypair,
        options: { showEffects: true },
    });
    if (result.effects?.status.status !== "success") throw new Error("Payment transaction failed");

    return fetch(requestUrl, {
        redirect: "manual",
        headers: {
            "X-Payment-Digest": result.digest,
            "X-Payment-Challenge": challenge,
            "X-Payment-Signature": challengeSignature,
        },
    });
}
// docs::/#fetch-with-payment

export { fetchWithPayment, parsePaymentInstructions };
export type { PaymentInstructions, PaymentPolicy };
