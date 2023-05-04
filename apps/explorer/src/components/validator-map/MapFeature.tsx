// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

interface Props {
    path: string | null;
}

export function MapFeature({ path }: Props) {
    if (!path) {
        return null;
    }

    return (
        <path
            d={path}
            fill="white"
            strokeWidth={0.2}
            stroke="var(--steel-dark)"
        />
    );
}
