// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';

import { type Feature } from './types';

interface Props {
    path: string | null;
    feature: Feature;
    onMouseOver(event: React.MouseEvent, countryCode?: string): void;
    onMouseOut(): void;
}

export function MapFeature({ path, feature, onMouseOver, onMouseOut }: Props) {
    const handleMouseOver = useCallback(
        (e: React.MouseEvent) => {
            onMouseOver(e, feature.properties.alpha2);
        },
        [feature.properties.alpha2, onMouseOver]
    );

    if (!path) {
        return null;
    }

    return (
        <path
            onMouseOver={handleMouseOver}
            onMouseMove={handleMouseOver}
            onMouseOut={onMouseOut}
            d={path}
            fill="white"
            strokeWidth={0.2}
            stroke="var(--steel)"
        />
    );
}
