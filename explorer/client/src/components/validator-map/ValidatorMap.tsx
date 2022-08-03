// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { ParentSizeModern } from '@visx/responsive';
import { Tooltip, useTooltip, useTooltipInPortal } from '@visx/tooltip';
import { localPoint } from '@visx/event';
import React, { useCallback, useMemo } from 'react';

import { ReactComponent as ForwardArrowDark } from '../../assets/SVGIcons/forward-arrow-dark.svg';
import { WorldMap, type NodeLocation } from './WorldMap';

import styles from './ValidatorMap.module.css';

const HOST = 'https://imgmod.sui.io';

type NodeList = [
    ip: string,
    city: string,
    region: string,
    country: string,
    loc: string
][];

const MOCK_DATA = [
    ['135.181.16.143', 'Tuusula', 'Uusimaa', 'Finland', '60.3540,24.9794'],
    [
        '167.99.148.152',
        'North Bergen',
        'New Jersey',
        'United States',
        '40.8043,-74.0121',
    ],
    [
        '195.46.164.179',
        'Kemerovo',
        'Kemerovo Oblast',
        'Russia',
        '55.3333,86.0833',
    ],
    ['23.88.34.250', 'Oberdorla', 'Thuringia', 'Germany', '51.1658,10.4216'],
    ['23.88.34.250', 'Oberdorla', 'Thuringia', 'Germany', '51.1658,10.4216'],
    ['23.88.34.250', 'Oberdorla', 'Thuringia', 'Germany', '51.1658,10.4216'],
    [
        '37.154.179.123',
        'Washington',
        'Washington',
        'United States',
        '41.0318,28.9684',
    ],
    [
        '51.38.194.32',
        'Gravelines',
        'Hauts-de-France',
        'France',
        '50.9865,2.1281',
    ],
    [
        '51.38.194.32',
        'Gravelines',
        'Hauts-de-France',
        'France',
        '50.9865,2.1281',
    ],
    [
        '51.38.194.32',
        'Gravelines',
        'Hauts-de-France',
        'France',
        '50.9865,2.1281',
    ],
    [
        '51.38.194.32',
        'Gravelines',
        'Hauts-de-France',
        'France',
        '50.9865,2.1281',
    ],
    [
        '51.38.194.32',
        'Gravelines',
        'Hauts-de-France',
        'France',
        '50.9865,2.1281',
    ],
    [
        '51.38.194.32',
        'Gravelines',
        'Hauts-de-France',
        'France',
        '50.9865,2.1281',
    ],
    ['8.21.13.47', 'Newark', 'New Jersey', 'United States', '40.7357,-74.1724'],
    ['8.21.13.47', 'Newark', 'New Jersey', 'United States', '40.7357,-74.1724'],
    ['8.21.13.47', 'Newark', 'New Jersey', 'United States', '40.7357,-74.1724'],
    ['8.21.13.47', 'Newark', 'New Jersey', 'United States', '40.7357,-74.1724'],
    ['8.21.13.47', 'Newark', 'New Jersey', 'United States', '40.7357,-74.1724'],
];

export function ValidatorMap() {
    const { data } = useQuery(['validator-map'], async () => {
        return MOCK_DATA;
        const res = await fetch(`${HOST}/locations`, {
            method: 'GET',
        });

        return res.json() as Promise<NodeList>;
    });

    const bucketedData = useMemo(() => {
        if (!data) {
            return [];
        }

        const nodeLocations: Record<string, NodeLocation> = {};

        data.forEach(([ip, city, region, country, loc]) => {
            const key = `${city}-${region}-${country}`;
            nodeLocations[key] ??= {
                count: 0,
                city,
                country,
                location: loc
                    .split(',')
                    .reverse()
                    .map((geo) => parseFloat(geo)) as [number, number],
            };
            nodeLocations[key].count += 1;
        });

        return Object.values(nodeLocations);
    }, [data]);

    const {
        tooltipData,
        tooltipLeft,
        tooltipTop,
        tooltipOpen,
        showTooltip,
        hideTooltip,
    } = useTooltip();

    // If you don't want to use a Portal, simply replace `TooltipInPortal` below with
    // `Tooltip` or `TooltipWithBounds` and remove `containerRef`
    const { containerRef, TooltipInPortal } = useTooltipInPortal({
        // use TooltipWithBounds
        // detectBounds: true,
        // when tooltip containers are scrolled, this will correctly update the Tooltip position
        // scroll: true,
    });

    const handleMouseOver = useCallback(
        (event: React.MouseEvent<SVGElement>) => {
            const owner = event.currentTarget.ownerSVGElement;
            if (!owner) return;

            const coords = localPoint(owner, event);

            if (!coords) return;

            showTooltip({
                tooltipLeft: coords.x,
                tooltipTop: coords.y,
                tooltipData: event.currentTarget.getAttribute('name') || '',
            });
        },
        [showTooltip]
    );

    return (
        <div className={styles.card}>
            <div className={styles.container}>
                <div className={styles.contents}>
                    <div>
                        <div className={styles.title}>Total Nodes</div>
                        <div className={styles.stat}>15,123</div>
                    </div>
                    <div>
                        <div className={styles.title}>Average APY</div>
                        <div className={styles.stat}>4.96%</div>
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

            <div className={styles.mapcontainer} ref={containerRef}>
                <div className={styles.map}>
                    <ParentSizeModern>
                        {(parent) => (
                            <WorldMap
                                onMouseOver={handleMouseOver}
                                onMouseOut={hideTooltip}
                                width={parent.width}
                                height={parent.height}
                                nodes={bucketedData}
                            />
                        )}
                    </ParentSizeModern>

                    {tooltipOpen && (
                        <TooltipInPortal
                            key={Math.random()}
                            top={tooltipTop}
                            left={tooltipLeft}
                            className={styles.tooltip}
                            style={{}}
                        >
                            <div className={styles.tipitem}>
                                <div>Nodes</div>
                                <div>10</div>
                            </div>
                            <div className={styles.tipdivider} />
                            <div>{tooltipData}</div>
                        </TooltipInPortal>
                    )}
                </div>
            </div>
        </div>
    );
}
