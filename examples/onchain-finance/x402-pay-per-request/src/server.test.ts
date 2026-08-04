// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { ChallengeStore, DurableDigestStore } from "./replay-store.js";

const tempDirs: string[] = [];

function createStores(maxPending?: number) {
    const directory = mkdtempSync(join(tmpdir(), "x402-"));
    tempDirs.push(directory);
    const path = join(directory, "digests.jsonl");
    return {
        challenges: new ChallengeStore(new DurableDigestStore(path), maxPending),
        path,
    };
}

afterEach(() => {
    for (const directory of tempDirs.splice(0)) {
        rmSync(directory, { recursive: true, force: true });
    }
});

describe("replay store", () => {
    it("atomically reserves a challenge and digest", () => {
        const { challenges } = createStores();
        challenges.issue("challenge-1", Date.now() + 60_000);
        const reservation = challenges.reserve("challenge-1", "digest-1");

        expect(() => challenges.reserve("challenge-1", "digest-2")).toThrow(/already used/);
        expect(() => {
            challenges.issue("challenge-2", Date.now() + 60_000);
            challenges.reserve("challenge-2", "digest-1");
        }).toThrow(/already used/);
        reservation.rollback();
    });

    it("rolls back both identifiers after failed verification", () => {
        const { challenges } = createStores();
        challenges.issue("challenge-1", Date.now() + 60_000);
        challenges.reserve("challenge-1", "digest-1").rollback();

        expect(() => challenges.reserve("challenge-1", "digest-1")).not.toThrow();
    });

    it("rejects expired challenges", () => {
        const { challenges } = createStores();
        challenges.issue("challenge-1", Date.now() - 1);

        expect(() => challenges.reserve("challenge-1", "digest-1")).toThrow(/expired/);
    });

    it("bounds pending challenges and removes expired entries", () => {
        const { challenges } = createStores(1);
        challenges.issue("challenge-1", 200, 100);
        expect(() => challenges.issue("challenge-2", 300, 100)).toThrow(/Too many/);
        expect(() => challenges.issue("challenge-2", 300, 201)).not.toThrow();
    });

    it("persists accepted digests without expiration", () => {
        const { challenges, path } = createStores();
        challenges.issue("challenge-1", Date.now() + 60_000);
        challenges.reserve("challenge-1", "digest-1").commit();

        const afterRestart = new ChallengeStore(new DurableDigestStore(path));
        afterRestart.issue("challenge-2", Date.now() + 60_000);
        expect(() => afterRestart.reserve("challenge-2", "digest-1")).toThrow(/already used/);
    });
});
