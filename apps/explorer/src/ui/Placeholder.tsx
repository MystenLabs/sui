// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export interface PlaceholderProps {
    width?: string;
    height?: string;
}

export function Placeholder({
    width = '100%',
    height = '1em',
}: PlaceholderProps) {
    return (
        <div
            className="animate-shimmer bg-placeholderShimmer h-[1em] w-full rounded-[3px] bg-[length:1000px_100%]"
            style={{ width, height }}
        />
    );
}
