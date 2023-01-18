// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from "@mysten/sui.js";

bcs.registerStructType('Assignment', {
    validator: 'address',
    goal: 'u8',
    epoch: 'u64'
});

bcs.registerStructType('Scorecard', {
    id: 'address',
    name: 'string',
    assignment: 'Assignment',
    score: 'u16',
    participation: 'u16',
    epoch: 'u64'
});

export default bcs;
