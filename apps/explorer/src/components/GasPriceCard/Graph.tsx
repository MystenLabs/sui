// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { scaleTime, scaleLinear } from '@visx/scale';
import { LinePath } from '@visx/shape';
import { useMemo } from 'react';

import { type EpochGasInfo } from './types';

export type GraphProps = {
    data: EpochGasInfo[];
    width: number;
    height: number;
    durationDays: number;
};
export function Graph({ data, width, height, durationDays }: GraphProps) {
    const notEmptyData = useMemo(
        () =>
            data?.filter(
                ({ referenceGasPrice, date }) =>
                    referenceGasPrice !== null && !!date
            ),
        // const a = new Date();
        // a.setUTCDate(a.getUTCDate() - 1);
        // const b = new Date();
        // b.setUTCDate(b.getUTCDate() - 2);
        // const c = new Date();
        // c.setUTCDate(c.getUTCDate() - 3);
        // return [
        //     { date: new Date(), referenceGasPrice: 300, epoch: 4 },
        //     { date: a, referenceGasPrice: 500, epoch: 3 },
        //     { date: b, referenceGasPrice: 150, epoch: 2 },
        //     { date: c, referenceGasPrice: 550, epoch: 1 },
        // ];
        [data]
    );
    const xScale = useMemo(() => {
        const minDate = new Date();
        minDate.setUTCDate(minDate.getUTCDate() - durationDays);
        return scaleTime<number>({
            domain: [minDate, new Date()],
        });
    }, [durationDays]);
    xScale.range([10, width - 10]);
    const yScale = useMemo(() => {
        const prices = [
            ...notEmptyData.map(({ referenceGasPrice }) =>
                Number(referenceGasPrice!)
            ),
        ];
        return scaleLinear<number>({
            domain: [Math.min(...prices), Math.max(...prices)],
        });
    }, [notEmptyData]);
    yScale.range([40, height - 40]);
    return (
        <svg width={width} height={height} className="stroke-steel-dark/80">
            <LinePath<EpochGasInfo>
                data={notEmptyData}
                x={(d) => xScale(d.date!.getTime())}
                y={(d) => yScale(Number(d.referenceGasPrice!))}
                width="1"
            />
        </svg>
    );
}
