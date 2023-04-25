// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LinePath } from '@visx/shape';

import { type EpochGasInfo } from './types';

export type GraphProps = {
    data: EpochGasInfo[];
    width: number;
    height: number;
};
export function Graph({ data, width, height }: GraphProps) {
    return (
        <svg width={width} height={height}>
            <LinePath data={data} />
        </svg>
    );
}
