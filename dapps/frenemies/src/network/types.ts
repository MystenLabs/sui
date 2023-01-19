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
    validator: SuiAddress;
    /** Goal: Friend, Neutal or Enemy */
    goal: Goal;
    /** Epoch this assignment is for */
    epoch: number;
};

/**
 * Scorecard object.
 * Follows the Move definition.
 * Received through the `register` transaction call.
 */
export type Scorecard = {
    id: SuiAddress;
    /** Globally unique name of the player */
    name: string;
    /** Current Assignment */
    assignment: Assignment;
    /** Accumulated score across epochs */
    score: number;
    /** Number of epochs for which the player received a score (even 0) */
    participation: number;
    /** Latest epoch for which assignment was recorded; but a score has not yet been assigned */
    epoch: number;
};

/**
 * An event emitted when Scorecard was updated.
 * Contains all necessary information to build a table.
 */
export type ScorecardUpdatedEvent = {
    /** Name of the player */
    player: string;
    /** Player's assignment for the epoch */
    assignment: Assignment;
    /** Player's total score after scoring `assignment` */
    totalScore: number;
    /** Score for the epoch. 0 if the player was not successful */
    epochScore: number;
};

/**
 * Leaderboard object holding information about top X (1000) participants.
 */
export type Leaderboard = {
    id: SuiAddress;
    /** Top SCORE_MAX (1000) scores; sorted in ASC order */
    topScores: Score[];
    /** Validator set sorted by stake in ascending order for each epoch */
    // redundant field as it gives no information directly
    // prev_epoch_stakes: { id: SuiAddress, size: number }
    /** Current epoch */
    epoch: number;
    /** Epoch where the competition began; */
    startEpoch: number;
};

/**
 * A single Score record in the Leaderboard.
 */
export type Score = {
    /** Name of the player (unique) */
    name: string;
    /** The score of the player */
    score: number;
    /** Number of epochs the player has participated in */
    participation: number;
};
