// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { closeSync, existsSync, fsyncSync, openSync, readFileSync, writeSync } from "node:fs";

// docs::#replay-store
interface SettledPaymentRecord {
    payer: string;
    settledAt: number;
}

interface PaymentReservation {
    commit(payer: string): void;
    rollback(): void;
}

/**
 * Durable record of every transaction digest this server has already accepted
 * payment for.
 *
 * Sui execution is idempotent by digest: resubmitting a settled transaction
 * returns the original result without moving funds again. Without this store a
 * client could resend a captured PAYMENT-SIGNATURE header and be served for
 * free forever, so accepted digests must never expire.
 */
class SettledPaymentStore {
    private readonly records = new Map<string, SettledPaymentRecord>();

    constructor(private readonly path: string) {
        if (!existsSync(path)) return;

        const lines = readFileSync(path, "utf-8").split("\n");
        for (const [index, line] of lines.entries()) {
            if (!line) continue;
            try {
                const record = JSON.parse(line) as SettledPaymentRecord & { digest: string };
                this.records.set(record.digest, record);
            } catch {
                // A process crash can truncate only the final append. Malformed
                // earlier lines indicate corruption and require recovery.
                if (index !== lines.length - 1) throw new Error("Corrupt payment log");
            }
        }
    }

    has(digest: string): boolean {
        return this.records.has(digest);
    }

    add(digest: string, record: SettledPaymentRecord): void {
        if (this.records.has(digest)) throw new Error("Payment already settled");

        const fd = openSync(this.path, "a");
        try {
            writeSync(fd, JSON.stringify({ digest, ...record }) + "\n");
            fsyncSync(fd);
        } finally {
            closeSync(fd);
        }
        this.records.set(digest, record);
    }
}

/**
 * Guards a payment digest for the duration of verification and settlement.
 *
 * `reserve` is synchronous and runs before the first `await`, so two concurrent
 * requests carrying the same payload cannot both reach settlement.
 */
class PaymentReservations {
    private readonly inFlight = new Set<string>();

    constructor(private readonly settled: SettledPaymentStore) {}

    reserve(digest: string): PaymentReservation {
        if (this.settled.has(digest) || this.inFlight.has(digest)) {
            throw new Error("Payment already used");
        }

        this.inFlight.add(digest);
        let active = true;

        return {
            commit: (payer: string) => {
                if (!active) return;
                this.settled.add(digest, { payer, settledAt: Date.now() });
                this.inFlight.delete(digest);
                active = false;
            },
            rollback: () => {
                if (!active) return;
                this.inFlight.delete(digest);
                active = false;
            },
        };
    }
}
// docs::/#replay-store

export { PaymentReservations, SettledPaymentStore };
export type { PaymentReservation, SettledPaymentRecord };
