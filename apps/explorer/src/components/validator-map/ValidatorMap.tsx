// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { ParentSizeModern } from '@visx/responsive';
import { TooltipWithBounds, useTooltip } from '@visx/tooltip';
import React, { useCallback, useMemo } from 'react';

import { WorldMap } from './WorldMap';
import { type NodeLocation } from './types';

import styles from './ValidatorMap.module.css';

import { DateFilter, useDateFilterState } from '~/ui/DateFilter';
import { Placeholder } from '~/ui/Placeholder';

const HOST = 'https://imgmod.sui.io';

type CountryNodes = Record<string, { count: number; country: string }>;

const regionNamesInEnglish = new Intl.DisplayNames('en', { type: 'region' });
const numberFormatter = new Intl.NumberFormat('en');

const DATE_FILTER_TO_WINDOW = {
    D: '1d',
    W: '1w',
    M: '1m',
    ALL: 'all',
};

export default function ValidatorMap() {
    const [dateFilter, setDateFilter] = useDateFilterState('D');

    const { data, isLoading, isSuccess } = useQuery(
        ['validator-map', dateFilter],
        async () => {
            const res = await fetch(
                `${HOST}/location?${new URLSearchParams({
                    version: 'v2',
                    window: DATE_FILTER_TO_WINDOW[dateFilter],
                })}`,
                {
                    method: 'GET',
                }
            );

            if (!res.ok) {
                throw new Error('Failed to fetch validator map data');
            }

            return res.json() as Promise<NodeLocation[]>;
        }
    );

    const { totalCount, countryCount, countryNodes } = useMemo<{
        totalCount: number | null;
        countryCount: number | null;
        countryNodes: CountryNodes;
    }>(() => {
        if (!data) {
            return { totalCount: null, countryCount: null, countryNodes: {} };
        }

        let totalCount = 0;
        const countryNodes: CountryNodes = {};

        data.forEach(({ count, country }) => {
            totalCount += count;
            countryNodes[country] ??= {
                count: 0,
                country,
            };
            countryNodes[country].count += 1;
        });

        return {
            totalCount,
            countryCount: Object.keys(countryNodes).length,
            countryNodes,
        };
    }, [data]);

    const {
        tooltipData,
        tooltipLeft,
        tooltipTop,
        tooltipOpen,
        showTooltip,
        hideTooltip,
    } = useTooltip<string>();

    const handleMouseOver = useCallback(
        (event: React.MouseEvent<SVGElement>, countryCode?: string) => {
            const owner = event.currentTarget.ownerSVGElement;
            if (!owner) return;

            const rect = owner.getBoundingClientRect();

            if (countryCode && countryNodes[countryCode]) {
                showTooltip({
                    tooltipLeft: event.clientX - rect.x,
                    tooltipTop: event.clientY - rect.y,
                    tooltipData: countryCode,
                });
            } else {
                hideTooltip();
            }
        },
        [showTooltip, countryNodes, hideTooltip]
    );

    return (
        <div data-testid="fullnode-map" className={styles.card}>
            <div className={styles.container}>
                <div className={styles.contents}>
                    <div>
                        <div className={styles.stat}>
                            {isLoading && (
                                <Placeholder width="59px" height="32px" />
                            )}
                            {isSuccess &&
                                countryCount &&
                                numberFormatter.format(countryCount)}
                        </div>
                        <div className={styles.title}>Countries</div>
                    </div>
                    <div>
                        <div className={styles.stat}>
                            {isLoading && (
                                <Placeholder width="59px" height="32px" />
                            )}
                            {
                                // Fetch received response with no errors and the value was not null
                                isSuccess &&
                                    totalCount &&
                                    numberFormatter.format(totalCount)
                            }
                        </div>
                        <div className={styles.title}>
                            Nodes on Devnet and Testnet
                        </div>
                    </div>
                </div>
            </div>

            <div className="absolute top-5 right-5 z-10">
                <DateFilter value={dateFilter} onChange={setDateFilter} />
            </div>

            <div className={styles.mapcontainer}>
                <div className={styles.map}>
                    <ParentSizeModern>
                        {(parent) => (
                            <WorldMap
                                nodes={data}
                                width={parent.width}
                                height={parent.height}
                                onMouseOver={handleMouseOver}
                                onMouseOut={hideTooltip}
                            />
                        )}
                    </ParentSizeModern>
                </div>
            </div>

            {tooltipOpen && tooltipData && (
                <TooltipWithBounds
                    top={tooltipTop}
                    left={tooltipLeft}
                    className={styles.tooltip}
                    // NOTE: Tooltip will un-style itself if we provide a style object:
                    style={{}}
                >
                    <div className={styles.tipitem}>
                        <div>Nodes</div>
                        <div>{countryNodes[tooltipData].count}</div>
                    </div>
                    <div className={styles.tipdivider} />
                    <div>
                        {regionNamesInEnglish.of(
                            countryNodes[tooltipData].country
                        ) || countryNodes[tooltipData].country}
                    </div>
                </TooltipWithBounds>
            )}
        </div>
    );
}
