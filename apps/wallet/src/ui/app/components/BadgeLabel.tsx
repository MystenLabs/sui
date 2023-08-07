// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { Text } from '_src/ui/app/shared/text';

type BadgeLabelProps = {
    label: ReactNode;
};

export function BadgeLabel({ label }: BadgeLabelProps) {
    return (
        <div className="bg-gray-40 rounded-2xl border border-solid border-gray-45 py-1 px-1.5">
            <Text variant="captionSmallExtra" color="steel-dark">
                {label}
            </Text>
        </div>
    );
}
