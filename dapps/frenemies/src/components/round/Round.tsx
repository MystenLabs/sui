// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Round number.
 *
 * Requires reading the SuiSystem object to get current epoch
 * minus the start round for the Frenemies game.
 */
function Round({ num }: { num: number }) {
    return (
        <div className="py-10">
            <h2 className="round text-center">ROUND {num}</h2>
        </div>
    );
}

export default Round;
