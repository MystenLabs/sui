// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { ParentSizeModern } from '@visx/responsive';
import { TooltipWithBounds, useTooltip } from '@visx/tooltip';
import React, { type ReactNode, useCallback, useMemo } from 'react';

import { WorldMap } from './WorldMap';
import { type NodeLocation } from './types';

import { Card } from '~/ui/Card';
import { DateFilter, useDateFilterState } from '~/ui/DateFilter';
import { Heading } from '~/ui/Heading';
import { Placeholder } from '~/ui/Placeholder';
import { Text } from '~/ui/Text';

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

function NodeStat({ title, children }: { title: string; children: ReactNode }) {
    return (
        <div className="space-y-1.5">
            <Heading variant="heading2/semibold" color="steel-darker">
                {children}
            </Heading>
            <Text variant="caption/semibold" color="steel-dark">
                {title}
            </Text>
        </div>
    );
}

interface Props {
    minHeight: number;
}

// NOTE: This component is lazy imported, so it needs to be default exported:
export default function NodeMap({ minHeight }: Props) {
    const [dateFilter, setDateFilter] = useDateFilterState('D');

    const { data, isLoading, isSuccess } = useQuery(
        ['node-map', dateFilter],
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
                throw new Error('Failed to fetch node map data');
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
        <Card spacing="none">
            <div
                data-testid="node-map"
                className="relative flex flex-col justify-end"
                style={{ minHeight }}
            >
                <div className="pointer-events-none relative z-10 flex flex-1 flex-col justify-between gap-8 p-6">
                    <Heading variant="heading4/semibold" color="steel-darker">
                        Sui Nodes
                    </Heading>

                    <div className="flex flex-col gap-8">
                        <NodeStat title="Countries">
                            {isLoading && (
                                <Placeholder width="60px" height="0.8em" />
                            )}
                            {isSuccess &&
                                countryCount &&
                                numberFormatter.format(countryCount)}
                        </NodeStat>

                        <NodeStat title="Nodes on Devnet and Testnet">
                            {isLoading && (
                                <Placeholder width="60px" height="0.8em" />
                            )}
                            {
                                // Fetch received response with no errors and the value was not null
                                isSuccess &&
                                    totalCount &&
                                    numberFormatter.format(totalCount)
                            }
                        </NodeStat>
                    </div>
                </div>

                <div className="absolute top-5 right-5 z-10">
                    <DateFilter value={dateFilter} onChange={setDateFilter} />
                </div>

                <div className="absolute inset-0 z-0 overflow-hidden">
                    <div className="pointer-events-none absolute inset-0 md:pointer-events-auto">
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
                        className="absolute z-40 hidden min-w-[100px] rounded-md bg-gray-100 p-2 font-sans text-xs text-white md:block"
                        // NOTE: Tooltip will un-style itself if we provide a style object:
                        style={{}}
                    >
                        <div className="flex items-center justify-between font-semibold uppercase">
                            <div>Nodes</div>
                            <div>{countryNodes[tooltipData].count}</div>
                        </div>
                        <div className="my-1 h-px bg-gray-90" />
                        <div>
                            {regionNamesInEnglish.of(
                                countryNodes[tooltipData].country
                            ) || countryNodes[tooltipData].country}
                        </div>
                    </TooltipWithBounds>
                )}
            </div>
        </Card>
    );
}
