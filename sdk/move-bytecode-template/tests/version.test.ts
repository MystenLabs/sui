// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from "vitest";
import { version } from "../pkg";

describe('MBF-Wasm Version', () => {
    it('should be 1', () => {
        expect(version()).toBeTruthy();
    })
});
