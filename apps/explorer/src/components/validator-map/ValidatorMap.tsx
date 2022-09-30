// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { ParentSizeModern } from '@visx/responsive';
import { TooltipWithBounds, useTooltip } from '@visx/tooltip';
import React, { useCallback, useMemo } from 'react';

import { WorldMap } from './WorldMap';
import { type NodeLocation } from './types';

import styles from './ValidatorMap.module.css';

// const HOST = 'https://imgmod.sui.io';
const HOST = 'http://localhost:9200';

type CountryNodes = Record<string, { count: number; country: string }>;

const regionNamesInEnglish = new Intl.DisplayNames('en', { type: 'region' });
const numberFormatter = new Intl.NumberFormat('en');

export default function ValidatorMap() {
    const { data } = useQuery(['validator-map'], async () => {
        const res = await fetch(`${HOST}/location?version=v2`, {
            method: 'GET',
        });

        if (!res.ok) {
            return [];
        }

        return res.json() as Promise<NodeLocation[]>;
    });

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
                        <div className={styles.title}>Total Nodes</div>
                        <div className={styles.stat}>
                            {totalCount &&
                                numberFormatter.format(totalCount)}
                        </div>
                    </div>
                    <div>
                        <div className={styles.title}>Total Countries</div>
                        <div className={styles.stat}>
                            {countryCount &&
                                numberFormatter.format(countryCount)}
                        </div>
                    </div>
                </div>
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
