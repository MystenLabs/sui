// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Provides BCS schema for the used Move types.
 * @module network/bcs
 */

import { bcs } from "@mysten/sui.js";

bcs.registerStructType('frenemies::Assignment', {
    validator: 'address',
    goal: 'u8',
    epoch: 'u64'
});

bcs.registerStructType('frenemies::Scorecard', {
    id: 'address',
    name: 'string',
    assignment: 'frenemies::Assignment',
    score: 'u16',
    participation: 'u16',
    epoch: 'u64'
});

bcs.registerStructType('frenemies::ScorecardUpdateEvent', {
    player: 'string',
    assignment: 'frenemies::Assignment',
    totalScore: 'u16',
    epochScore: 'u16',
});

bcs.registerStructType('leaderboard::Leaderboard', {
    id: 'address',
    topScores: 'vector<Score>',
    prevEpochStakes: 'table::Table',
    epoch: 'u64',
    startEpoch: 'u64'
});

bcs.registerStructType('leaderboard::Score', {
    name: 'string',
    score: 'u16',
    participation: 'u16'
});

// This type only contains utility data;
// Other fields (based on generics) are attached as dynamic fields.
bcs.registerStructType('table::Table', {
    id: 'address',
    size: 'u64'
});

export default bcs;
