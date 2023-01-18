//// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Round number.
 *
 * Requires reading the SuiSystem object to get current epoch
 * minus the start round for the Frenemies game.
 */
function Block({ title, value }: { title: string, value: string }) {
    return (
        <div className="flex-auto py-4 text-center">
            <p>{title}</p>
            <p>{value}</p>
        </div>
    );
}

export default Block;
