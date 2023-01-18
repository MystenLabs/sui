// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider, Network, ObjectOwner, ObjectStatus, SuiObjectRef } from "@mysten/sui.js";
import { Scorecard } from "./types";
import bcs from "./bcs";

const provider = new JsonRpcProvider(Network.LOCAL);
const SCORECARD_TYPE = 'frenemies::Scorecard';

/**
 * Filling in the missing piece in TS SDK.
 */
export type RawObjectResponse = {
    status: ObjectStatus;
    details: {
        reference: SuiObjectRef;
        owner: ObjectOwner;
        data: {
            /* ... some other fields */
            bcs_bytes: string
        },
    }
};

/**
 * Scorecard fetching result.
 * Contains both the reference to use in txs and the data.
 */
export type ScorecardData = {
    reference: SuiObjectRef;
    scorecard: Scorecard;
};

/**
 * Get a Scorecard for an account if this account has at least one.
 *
 * We do not guarantee correct behavior if people registered more than once,
 * lookup is done with `Array.prototype.find` for the first occurrence.
 */
export async function getScorecard(account: string): Promise<ScorecardData | null> {
    const objects = await provider.getObjectsOwnedByAddress(account);
    const search = objects.find((v) => v.type.includes(SCORECARD_TYPE));

    if (!search) {
        return null;
    }

    const scorecard: RawObjectResponse = await provider.call('sui_getRawObject', [ search.objectId ]);
    const { reference, data: { bcs_bytes } } = scorecard.details;

    return {
        reference,
        scorecard: bcs.de('Scorecard', bcs_bytes, 'base64')
    };
}
