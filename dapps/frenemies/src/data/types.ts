// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress } from "@mysten/sui.js"

/**
 * Goal enum defined in the Assignment
 */
export enum Goal {
    /** Goal: validator finishes in top third by stake */
    Friend = 0,
    /** Goal: validator finishes in middle third by stake */
    Neutral = 1,
    /** Goal: validator finishes in bottom third by stake */
    Enemy = 2
};

/**
 * Assignment object - one per epoch.
 * Received through updating the Scorecard.
 */
export type Assignment = {
    /** Current assignment */
    validator: SuiAddress,
    /** Goal: Friend, Neutal or Enemy */
    goal: Goal,
    /** Epoch this assignment is for */
    epoch: number
};

/**
 * Scorecard object.
 * Follows the Move definition.
 * Received through the `register` transaction call.
 */
export type Scorecard = {
    id: SuiAddress,
    /** Globally unique name of the player */
    name: string,
    /** Current Assignment */
    assignment: Assignment,
    /** Accumulated score across epochs */
    score: number,
    /** Number of epochs for which the player received a score (even 0) */
    participation: number,
    /** Latest epoch for which assignment was recorded; but a score has not yet been assigned */
    epoch: number
};
