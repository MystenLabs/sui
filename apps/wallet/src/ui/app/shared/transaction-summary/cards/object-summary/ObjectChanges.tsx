// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type ObjectChangeSummary } from '@mysten/core';

import { ObjectChangeEntry } from './ObjectChangeEntry';

export function ObjectChanges({
    changes,
}: {
    changes?: ObjectChangeSummary | null;
}) {
    if (!changes) return null;
    return (
        <>
            {Object.entries(changes).map(([type, changes]) => (
                <ObjectChangeEntry
                    type={type as keyof ObjectChangeSummary}
                    changes={changes}
                />
            ))}
        </>
    );
}
