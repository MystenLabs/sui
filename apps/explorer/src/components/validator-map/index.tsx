// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { ParentSize } from '@visx/responsive';
import { TooltipWithBounds, useTooltip } from '@visx/tooltip';
import React, { type ReactNode, useCallback, useMemo } from 'react';

import { WorldMap } from './WorldMap';
import { type ValidatorWithLocation } from './types';

import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Placeholder } from '~/ui/Placeholder';
import { Text } from '~/ui/Text';

const HOST = 'https://imgmod.sui.io';

type ValidatorsMap = Record<string, ValidatorWithLocation>;

const numberFormatter = new Intl.NumberFormat('en');

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
    minHeight: string | number;
}

// NOTE: This component is lazy imported, so it needs to be default exported:
export default function ValidatorMap({ minHeight }: Props) {

    const { data, isLoading, isSuccess } = useQuery(
        ['validator-map'],
        async () => {
            const res = await fetch(
                `http://localhost:3003/validator-map-v2?network=testnet`,
                {
                    method: 'GET',
                }
            );

            if (!res.ok) {
                throw new Error('Failed to fetch validator map data');
            }
            return res.json() as Promise<(ValidatorWithLocation | null)[]>;
        }
    );

    const { totalCount, countryCount, validatorMap } = useMemo<{
        totalCount: number | null;
        countryCount: number | null;
        validatorMap: ValidatorsMap;
    }>(() => {
        if (!data) {
            return { totalCount: null, countryCount: null, validatorMap: {} };
        }

        let totalCount = 0;
        const validatorMap: ValidatorsMap = {};
        const countryMap: Record<string, number> = {}
        data.forEach((validator) => {
            if (validator) {
                totalCount++;
                validatorMap[validator.suiAddress] ??= {
                    ...validator
                };

                if (countryMap[validator.country]) {
                    countryMap[validator.country]++;
                } else {
                    countryMap[validator.country] = 1;
                }
            }
        });

        return {
            totalCount,
            countryCount: Object.keys(countryMap).length,
            validatorMap,
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
        (event: React.MouseEvent<SVGElement>, validator?: string) => {
            const owner = event.currentTarget.ownerSVGElement;

            if (!owner) return;

            const rect = owner.getBoundingClientRect();

            if (validator) {
                showTooltip({
                    tooltipLeft: event.clientX - rect.x,
                    tooltipTop: event.clientY - rect.y,
                    tooltipData: validator,
                });
            } else {
                hideTooltip();
            }
        },
        [showTooltip, hideTooltip]
    );

    return (
        <Card height="full" spacing="none">
            <div
                data-testid="node-map"
                className="relative flex flex-col justify-end"
                style={{ minHeight }}
            >
                <div className="pointer-events-none relative z-10 flex flex-1 flex-col justify-between gap-8 p-6">
                    <Heading variant="heading4/semibold" color="steel-darker">
                        countries
                        {isLoading && (
                            <Placeholder width="60px" height="0.8em" />
                        )}
                        {isSuccess &&
                            countryCount &&
                            numberFormatter.format(countryCount)}
                    </Heading>

                    <div className="flex gap-6">
                        <NodeStat title="Validators">
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

                <div className="absolute inset-0 z-0 overflow-hidden">
                    <div className="pointer-events-none absolute inset-0 md:pointer-events-auto">
                        <ParentSize>
                            {(parent) => (
                                <WorldMap
                                    validators={data}
                                    width={parent.width}
                                    height={parent.height}
                                    onMouseOver={handleMouseOver}
                                    onMouseOut={hideTooltip}
                                />
                            )}
                        </ParentSize>
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
                        <div className="flex flex-col justify-start font-semibold">
                            <div>{validatorMap[tooltipData].name}</div>
                            <Text variant="pSubtitleSmall/normal" color="gray-60">{validatorMap[tooltipData].city}, {validatorMap[tooltipData].country}</Text>
                        </div>
                        <div className="my-1 h-px bg-gray-90" />
                        <div className="min-w-[120px]">
                            <div className="flex justify-between"><Text variant="subtitle/medium">Voting Power</Text>
                            <Text variant="subtitle/medium">
                            {Number(validatorMap[tooltipData].votingPower) / 100}%</Text></div>
                        </div>
                    </TooltipWithBounds>
                )}
            </div>
        </Card>
    );
}
