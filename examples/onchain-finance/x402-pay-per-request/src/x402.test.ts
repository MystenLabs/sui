// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { selectPaymentRequirements } from "./client.js";
import type { PaymentPolicy } from "./client.js";
import { PaymentReservations, SettledPaymentStore } from "./replay-store.js";
import {
    X402_VERSION,
    decodeHeaderValue,
    encodeHeaderValue,
    parsePaymentPayload,
    parsePaymentRequired,
} from "./x402.js";
import type { PaymentRequired, PaymentRequirements } from "./x402.js";

const tempDirs: string[] = [];

function createStore() {
    const directory = mkdtempSync(join(tmpdir(), "x402-"));
    tempDirs.push(directory);
    const path = join(directory, "settled.jsonl");
    return { reservations: new PaymentReservations(new SettledPaymentStore(path)), path };
}

const PAY_TO = "0x1eb7c57e3f2bd0fc6cb9dcffd143ea957e4d98f805c358733f76dee0667fe0b1";

function requirements(overrides: Partial<PaymentRequirements> = {}): PaymentRequirements {
    return {
        scheme: "exact",
        network: "sui:mainnet",
        amount: "1000000",
        asset: "0x2::sui::SUI",
        payTo: PAY_TO,
        maxTimeoutSeconds: 60,
        ...overrides,
    };
}

function paymentRequired(overrides: Partial<PaymentRequired> = {}): PaymentRequired {
    return {
        x402Version: X402_VERSION,
        resource: { url: "https://api.example.com/api/resource" },
        accepts: [requirements()],
        ...overrides,
    };
}

afterEach(() => {
    for (const directory of tempDirs.splice(0)) {
        rmSync(directory, { recursive: true, force: true });
    }
});

describe("replay store", () => {
    it("reserves a digest exactly once while it is in flight", () => {
        const { reservations } = createStore();
        const reservation = reservations.reserve("digest-1");

        expect(() => reservations.reserve("digest-1")).toThrow(/already used/);
        reservation.rollback();
    });

    it("frees the digest after a rolled back verification", () => {
        const { reservations } = createStore();
        reservations.reserve("digest-1").rollback();

        expect(() => reservations.reserve("digest-1")).not.toThrow();
    });

    it("persists settled payments across a restart, without expiry", () => {
        const { reservations, path } = createStore();
        reservations.reserve("digest-1").commit(PAY_TO);

        const afterRestart = new PaymentReservations(new SettledPaymentStore(path));
        expect(() => afterRestart.reserve("digest-1")).toThrow(/already used/);
        expect(() => afterRestart.reserve("digest-2")).not.toThrow();
    });

    it("tolerates a truncated final append but rejects earlier corruption", () => {
        const { reservations, path } = createStore();
        reservations.reserve("digest-1").commit(PAY_TO);

        writeFileSync(path, `${'{"digest":"digest-1","payer":"a","settledAt":1}'}\n{"digest":"tr`);
        expect(() => new SettledPaymentStore(path)).not.toThrow();

        writeFileSync(path, `{"digest":"trunc\n{"digest":"digest-1","payer":"a","settledAt":1}\n`);
        expect(() => new SettledPaymentStore(path)).toThrow(/Corrupt/);
    });
});

describe("http transport codec", () => {
    it("round-trips a payment required object through a header value", () => {
        const encoded = encodeHeaderValue(paymentRequired());
        expect(encoded).not.toContain("{");

        const decoded = parsePaymentRequired(decodeHeaderValue(encoded));
        expect(decoded.accepts[0]?.payTo).toBe(PAY_TO);
    });

    it("rejects an unsupported protocol version", () => {
        const encoded = encodeHeaderValue({ ...paymentRequired(), x402Version: 1 });
        expect(() => parsePaymentRequired(decodeHeaderValue(encoded))).toThrow(/version/);
    });

    it("rejects a payload without a signed transaction", () => {
        const encoded = encodeHeaderValue({
            x402Version: X402_VERSION,
            accepted: requirements(),
            payload: { signature: "abc" },
        });
        expect(() => parsePaymentPayload(decodeHeaderValue(encoded))).toThrow(/Invalid payment/);
    });

    it("rejects a non-numeric amount", () => {
        const encoded = encodeHeaderValue(
            paymentRequired({ accepts: [requirements({ amount: "1e6" })] }),
        );
        expect(() => parsePaymentRequired(decodeHeaderValue(encoded))).toThrow(/requirements/);
    });
});

describe("client payment policy", () => {
    const policy: PaymentPolicy = {
        expectedOrigin: "https://api.example.com",
        expectedPayTo: PAY_TO,
        allowedNetworks: ["sui:mainnet"],
        allowedAssets: ["0x2::sui::SUI"],
        maxAmount: 5_000_000n,
    };

    it("selects the first requirement that satisfies the policy", () => {
        const selected = selectPaymentRequirements(
            paymentRequired({
                accepts: [requirements({ network: "sui:testnet" }), requirements()],
            }),
            policy,
        );
        expect(selected.network).toBe("sui:mainnet");
    });

    it("refuses a recipient the policy does not name", () => {
        const other = `0x${"9".repeat(64)}`;
        expect(() =>
            selectPaymentRequirements(
                paymentRequired({ accepts: [requirements({ payTo: other })] }),
                policy,
            ),
        ).toThrow(/policy/);
    });

    it("refuses an amount above the cap and an asset outside the allowlist", () => {
        expect(() =>
            selectPaymentRequirements(
                paymentRequired({ accepts: [requirements({ amount: "5000001" })] }),
                policy,
            ),
        ).toThrow(/policy/);

        expect(() =>
            selectPaymentRequirements(
                paymentRequired({ accepts: [requirements({ asset: "0x2::foo::FOO" })] }),
                policy,
            ),
        ).toThrow(/policy/);
    });
});
