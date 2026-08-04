// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Types and helpers for x402 protocol version 2.
//
// Layering follows the specification: transport-independent types (section 5),
// the `exact` scheme payload for Sui (specs/schemes/exact/scheme_exact_sui.md),
// and the HTTP transport representation (specs/transports-v2/http.md).

import { fromBase64, toBase64 } from "@mysten/sui/utils";

// docs::#protocol-types
const X402_VERSION = 2;

/** Payment scheme identifier. Sui payments use the `exact` scheme. */
const EXACT_SCHEME = "exact";

/** Describes the protected resource (PaymentRequired.resource). */
interface ResourceInfo {
    url: string;
    description?: string;
    mimeType?: string;
}

/** One acceptable way to pay for a resource (PaymentRequired.accepts[n]). */
interface PaymentRequirements {
    scheme: string;
    /** CAIP-2 network identifier, for example `sui:mainnet`. */
    network: string;
    /** Amount in atomic units. SUI is denominated in MIST. */
    amount: string;
    /** Coin type for Sui, for example `0x2::sui::SUI`. */
    asset: string;
    payTo: string;
    maxTimeoutSeconds: number;
    extra?: Record<string, unknown>;
}

/** Body of the 402 response (carried in the PAYMENT-REQUIRED header). */
interface PaymentRequired {
    x402Version: number;
    error?: string;
    resource: ResourceInfo;
    accepts: PaymentRequirements[];
    extensions?: Record<string, unknown>;
}

/**
 * Scheme payload for `exact` on Sui: a complete transaction signed by the
 * payer. The resource server cannot redirect the funds, because changing any
 * byte of the transaction invalidates the signature.
 */
interface SuiExactPayload {
    /** Base64 Sui signature over the transaction. */
    signature: string;
    /** Base64 BCS-encoded transaction data. */
    transaction: string;
}

/** Payment authorization sent by the client (PAYMENT-SIGNATURE header). */
interface PaymentPayload {
    x402Version: number;
    resource?: ResourceInfo;
    accepted: PaymentRequirements;
    payload: SuiExactPayload;
    extensions?: Record<string, unknown>;
}

/** Settlement outcome returned by the server (PAYMENT-RESPONSE header). */
interface SettlementResponse {
    success: boolean;
    errorReason?: X402ErrorReason;
    payer?: string;
    /** Transaction digest, or an empty string when settlement failed. */
    transaction: string;
    network: string;
    amount?: string;
    extensions?: Record<string, unknown>;
}

/** Error codes defined in section 9 of the specification. */
type X402ErrorReason =
    | "insufficient_funds"
    | "invalid_network"
    | "invalid_payload"
    | "invalid_payment_requirements"
    | "invalid_scheme"
    | "invalid_transaction_state"
    | "invalid_x402_version"
    | "unexpected_settle_error"
    | "unexpected_verify_error";
// docs::/#protocol-types

// docs::#http-transport
/** HTTP transport header names (lowercase; HTTP header names are case-insensitive). */
const PAYMENT_REQUIRED_HEADER = "payment-required";
const PAYMENT_SIGNATURE_HEADER = "payment-signature";
const PAYMENT_RESPONSE_HEADER = "payment-response";

/**
 * A signed Sui transaction is large enough that the encoded header can exceed
 * the 8 KiB default many HTTP stacks allow per header. Reject anything larger
 * than this before decoding so a malicious client cannot force the server to
 * allocate an arbitrary buffer.
 */
const MAX_HEADER_BYTES = 32 * 1024;

function encodeHeaderValue(value: unknown): string {
    const encoded = toBase64(new TextEncoder().encode(JSON.stringify(value)));
    if (encoded.length > MAX_HEADER_BYTES) throw new Error("Encoded header exceeds size limit");
    return encoded;
}

function decodeHeaderValue(value: string): unknown {
    if (value.length > MAX_HEADER_BYTES) throw new Error("Encoded header exceeds size limit");
    return JSON.parse(new TextDecoder().decode(fromBase64(value)));
}
// docs::/#http-transport

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseResourceInfo(value: unknown): ResourceInfo {
    if (!isRecord(value) || typeof value.url !== "string") {
        throw new Error("Invalid resource info");
    }
    return {
        url: value.url,
        ...(typeof value.description === "string" ? { description: value.description } : {}),
        ...(typeof value.mimeType === "string" ? { mimeType: value.mimeType } : {}),
    };
}

/** Validates the shape of a PaymentRequirements object. Values are checked by the caller. */
function parsePaymentRequirements(value: unknown): PaymentRequirements {
    if (
        !isRecord(value) ||
        typeof value.scheme !== "string" ||
        typeof value.network !== "string" ||
        typeof value.amount !== "string" ||
        !/^[0-9]+$/.test(value.amount) ||
        typeof value.asset !== "string" ||
        typeof value.payTo !== "string" ||
        typeof value.maxTimeoutSeconds !== "number" ||
        !Number.isInteger(value.maxTimeoutSeconds) ||
        value.maxTimeoutSeconds <= 0
    ) {
        throw new Error("Invalid payment requirements");
    }
    return {
        scheme: value.scheme,
        network: value.network,
        amount: value.amount,
        asset: value.asset,
        payTo: value.payTo,
        maxTimeoutSeconds: value.maxTimeoutSeconds,
        ...(isRecord(value.extra) ? { extra: value.extra } : {}),
    };
}

/** Decodes and validates a PAYMENT-REQUIRED header. */
function parsePaymentRequired(value: unknown): PaymentRequired {
    if (!isRecord(value)) throw new Error("Invalid payment required response");
    if (value.x402Version !== X402_VERSION) throw new Error("Unsupported x402 version");
    if (!Array.isArray(value.accepts) || value.accepts.length === 0) {
        throw new Error("Payment required response lists no payment methods");
    }
    return {
        x402Version: X402_VERSION,
        ...(typeof value.error === "string" ? { error: value.error } : {}),
        resource: parseResourceInfo(value.resource),
        accepts: value.accepts.map(parsePaymentRequirements),
    };
}

/** Decodes and validates a PAYMENT-SIGNATURE header. */
function parsePaymentPayload(value: unknown): PaymentPayload {
    if (!isRecord(value)) throw new Error("Invalid payment payload");
    if (value.x402Version !== X402_VERSION) throw new Error("Unsupported x402 version");
    if (
        !isRecord(value.payload) ||
        typeof value.payload.signature !== "string" ||
        typeof value.payload.transaction !== "string"
    ) {
        throw new Error("Invalid payment payload");
    }
    return {
        x402Version: X402_VERSION,
        ...(value.resource === undefined ? {} : { resource: parseResourceInfo(value.resource) }),
        accepted: parsePaymentRequirements(value.accepted),
        payload: {
            signature: value.payload.signature,
            transaction: value.payload.transaction,
        },
    };
}

/** Decodes and validates a PAYMENT-RESPONSE header. */
function parseSettlementResponse(value: unknown): SettlementResponse {
    if (
        !isRecord(value) ||
        typeof value.success !== "boolean" ||
        typeof value.transaction !== "string" ||
        typeof value.network !== "string"
    ) {
        throw new Error("Invalid settlement response");
    }
    return {
        success: value.success,
        transaction: value.transaction,
        network: value.network,
        ...(typeof value.errorReason === "string"
            ? { errorReason: value.errorReason as X402ErrorReason }
            : {}),
        ...(typeof value.payer === "string" ? { payer: value.payer } : {}),
        ...(typeof value.amount === "string" ? { amount: value.amount } : {}),
    };
}

export {
    EXACT_SCHEME,
    MAX_HEADER_BYTES,
    PAYMENT_REQUIRED_HEADER,
    PAYMENT_RESPONSE_HEADER,
    PAYMENT_SIGNATURE_HEADER,
    X402_VERSION,
    decodeHeaderValue,
    encodeHeaderValue,
    parsePaymentPayload,
    parsePaymentRequired,
    parsePaymentRequirements,
    parseSettlementResponse,
};
export type {
    PaymentPayload,
    PaymentRequired,
    PaymentRequirements,
    ResourceInfo,
    SettlementResponse,
    SuiExactPayload,
    X402ErrorReason,
};
