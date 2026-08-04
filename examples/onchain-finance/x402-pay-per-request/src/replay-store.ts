// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { closeSync, existsSync, fsyncSync, openSync, readFileSync, writeSync } from "node:fs";

interface PendingChallenge {
    expiry: number;
    reservedDigest?: string;
}

interface UsedDigestRecord {
    challengeId: string;
    acceptedAt: number;
}

interface PaymentReservation {
    commit(): void;
    rollback(): void;
}

class DurableDigestStore {
    private readonly records = new Map<string, UsedDigestRecord>();

    constructor(private readonly path: string) {
        if (!existsSync(path)) return;

        const lines = readFileSync(path, "utf8").split("\n");
        for (const [index, line] of lines.entries()) {
            if (!line) continue;
            try {
                const record = JSON.parse(line) as UsedDigestRecord & { digest: string };
                this.records.set(record.digest, record);
            } catch {
                // A process crash can truncate only the final append. Malformed
                // earlier lines indicate corruption and require recovery.
                if (index !== lines.length - 1) throw new Error("Corrupt digest replay log");
            }
        }
    }

    has(digest: string): boolean {
        return this.records.has(digest);
    }

    add(digest: string, record: UsedDigestRecord): void {
        if (this.records.has(digest)) throw new Error("Payment digest already used");

        const fd = openSync(this.path, "a");
        try {
            writeSync(fd, `${JSON.stringify({ digest, ...record })}\n`);
            fsyncSync(fd);
        } finally {
            closeSync(fd);
        }
        this.records.set(digest, record);
    }
}

class ChallengeStore {
    private readonly pending = new Map<string, PendingChallenge>();
    private readonly reservedDigests = new Set<string>();

    constructor(
        private readonly usedDigests: DurableDigestStore,
        private readonly maxPending = 10_000,
    ) {}

    issue(id: string, expiry: number, now = Date.now()): void {
        this.sweepExpired(now);
        if (this.pending.size >= this.maxPending) throw new Error("Too many pending challenges");
        this.pending.set(id, { expiry });
    }

    sweepExpired(now = Date.now()): void {
        for (const [id, challenge] of this.pending) {
            if (!challenge.reservedDigest && now > challenge.expiry) this.pending.delete(id);
        }
    }

    reserve(challengeId: string, digest: string, now = Date.now()): PaymentReservation {
        const challenge = this.pending.get(challengeId);
        if (!challenge || now > challenge.expiry) {
            this.pending.delete(challengeId);
            throw new Error("Invalid or expired challenge");
        }
        if (
            challenge.reservedDigest ||
            this.reservedDigests.has(digest) ||
            this.usedDigests.has(digest)
        ) {
            throw new Error("Challenge or payment digest already used");
        }

        challenge.reservedDigest = digest;
        this.reservedDigests.add(digest);
        let active = true;

        return {
            commit: () => {
                if (!active) return;
                this.usedDigests.add(digest, { challengeId, acceptedAt: Date.now() });
                this.pending.delete(challengeId);
                this.reservedDigests.delete(digest);
                active = false;
            },
            rollback: () => {
                if (!active) return;
                const current = this.pending.get(challengeId);
                if (current?.reservedDigest === digest) delete current.reservedDigest;
                this.reservedDigests.delete(digest);
                active = false;
            },
        };
    }
}

export { ChallengeStore, DurableDigestStore };
export type { PaymentReservation, PendingChallenge, UsedDigestRecord };
