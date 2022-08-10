// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { ParentSizeModern } from '@visx/responsive';
import { TooltipWithBounds, useTooltip } from '@visx/tooltip';
import React, { useCallback, useMemo } from 'react';

import { ReactComponent as ForwardArrowDark } from '../../assets/SVGIcons/forward-arrow-dark.svg';
import { WorldMap } from './WorldMap';
import { type NodeLocation } from './types';

import styles from './ValidatorMap.module.css';

const HOST = 'https://imgmod.sui.io';

type NodeList = {
    ip_address: string;
    city: string;
    region: string;
    country: string;
    location: string;
}[];

type CountryNodes = Record<string, { count: number; countryCode: string }>;

const regionNamesInEnglish = new Intl.DisplayNames(['en'], { type: 'region' });

export default function ValidatorMap() {
    const { data } = useQuery(['validator-map'], async () => {
        const res = await fetch(`${HOST}/location`, {
            method: 'GET',
        });

        if (!res.ok) {
            return [];
        }

        return res.json() as Promise<NodeList>;
    });

    const { nodes, countryCount, countryNodes } = useMemo<{
        nodes: NodeLocation[];
        countryCount?: number;
        countryNodes: CountryNodes;
    }>(() => {
        if (!data) {
            return { nodes: [], countryNodes: {} };
        }

        const nodeLocations: Record<string, NodeLocation> = {};
        const countryNodes: CountryNodes = {};

        data.forEach(({ city, region, country, location }) => {
            const key = `${city}-${region}-${country}`;

            countryNodes[country] ??= {
                count: 0,
                countryCode: country,
            };
            countryNodes[country].count += 1;

            nodeLocations[key] ??= {
                count: 0,
                city,
                countryCode: country,
                location: location
                    .split(',')
                    .reverse()
                    .map((geo) => parseFloat(geo)) as [number, number],
            };
            nodeLocations[key].count += 1;
        });

        return {
            nodes: Object.values(nodeLocations),
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
        <div className={styles.card}>
            <div className={styles.container}>
                <div className={styles.contents}>
                    <div>
                        <div className={styles.title}>Total Nodes</div>
                        <div className={styles.stat}>{data?.length}</div>
                    </div>
                    <div>
                        <div className={styles.title}>Total Countries</div>
                        <div className={styles.stat}>{countryCount}</div>
                    </div>
                </div>

                <a
                    href="https://bit.ly/sui_validator"
                    className={styles.button}
                >
                    <span>Become a Validator</span>
                    <ForwardArrowDark />
                </a>
            </div>

            <div className={styles.mapcontainer}>
                <div className={styles.map}>
                    <ParentSizeModern>
                        {(parent) => (
                            <WorldMap
                                nodes={nodes}
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
                            countryNodes[tooltipData].countryCode
                        ) || countryNodes[tooltipData].countryCode}
                    </div>
                </TooltipWithBounds>
            )}
        </div>
    );
}
