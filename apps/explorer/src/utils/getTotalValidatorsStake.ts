// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type MoveActiveValidator } from '@mysten/sui.js';

export function getTotalValidatorsStake(validators: MoveActiveValidator[]) {
    return validators.reduce(
        (acc, curr) =>
            (acc +=
                BigInt(curr.fields.delegation_staking_pool.fields.sui_balance) +
                BigInt(curr.fields.stake_amount)),
        0n
    );
}
