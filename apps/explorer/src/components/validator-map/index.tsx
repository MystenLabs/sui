// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetSystemState, useAppsBackend } from '@mysten/core';
import { Heading, Text, Placeholder } from '@mysten/ui';
import { useQuery } from '@tanstack/react-query';
import { ParentSize } from '@visx/responsive';
import { TooltipWithBounds, useTooltip } from '@visx/tooltip';
import React, { type ReactNode, useCallback, useMemo } from 'react';

import { WorldMap } from './WorldMap';
import { type ValidatorMapResponse, type ValidatorMapValidator } from './types';
import { useNetwork } from '~/context';
import { Card } from '~/ui/Card';
import { Network } from '~/utils/api/DefaultRpcClient';

type ValidatorsMap = Record<string, ValidatorMapValidator>;

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
	const [network] = useNetwork();
	const { data: systemState, isError: systemStateError } = useGetSystemState();

	const { request } = useAppsBackend();

	const { data, isLoading, isError } = useQuery({
		queryKey: ['validator-map'],
		queryFn: () =>
			request<ValidatorMapResponse>('validator-map', {
				network: network.toLowerCase(),
				version: '2',
			}),
	});

	const validatorData = data?.validators;

	const { countryCount, validatorMap } = useMemo<{
		countryCount: number | null;
		validatorMap: ValidatorsMap;
	}>(() => {
		if (!validatorData) {
			return { totalCount: null, countryCount: null, validatorMap: {} };
		}

		const validatorMap: ValidatorsMap = {};
		const countryMap: Record<string, number> = {};
		validatorData.forEach((validator) => {
			if (validator) {
				validatorMap[validator.suiAddress] ??= {
					...validator,
				};

				if (validator.ipInfo) {
					if (countryMap[validator.ipInfo.country]) {
						countryMap[validator.ipInfo.country]++;
					} else {
						countryMap[validator.ipInfo.country] = 1;
					}
				}
			}
		});

		return {
			countryCount: Object.keys(countryMap).length,
			validatorMap,
		};
	}, [validatorData]);

	const { tooltipData, tooltipLeft, tooltipTop, tooltipOpen, showTooltip, hideTooltip } =
		useTooltip<string>();

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
		[showTooltip, hideTooltip],
	);

	return (
		<Card height="full" spacing="none">
			<div
				data-testid="node-map"
				className="relative flex flex-col justify-end"
				style={{ minHeight }}
			>
				<div className="pointer-events-none relative z-10 flex flex-1 flex-col justify-between gap-8 p-6">
					<div className="flex flex-col gap-2">
						<Text variant="caption/medium" color="steel-darker">
							Countries
							{isLoading && <Placeholder width="60px" height="0.8em" />}
						</Text>
						<Text variant="body/bold" color="steel-darker">
							{(!isError && countryCount && numberFormatter.format(countryCount)) || '--'}
						</Text>
					</div>

					<div className="flex gap-6">
						<NodeStat title="Validators">
							{isLoading && <Placeholder width="60px" height="0.8em" />}
							{
								// Fetch received response with no errors and the value was not null
								(!systemStateError &&
									systemState &&
									numberFormatter.format(systemState.activeValidators.length)) ||
									'--'
							}
						</NodeStat>

						{network === Network.MAINNET && (
							<NodeStat title="Nodes">
								{isLoading && <Placeholder width="60px" height="0.8em" />}
								{(data?.nodeCount && numberFormatter.format(data?.nodeCount)) || '--'}
							</NodeStat>
						)}
					</div>
				</div>

				<div className="absolute inset-0 z-0 overflow-hidden">
					<div className="pointer-events-none absolute inset-0 md:pointer-events-auto">
						<ParentSize>
							{(parent) => (
								<WorldMap
									validators={validatorData}
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
						{validatorMap[tooltipData].ipInfo && (
							<div className="flex flex-col justify-start font-semibold">
								<div>{validatorMap[tooltipData].name}</div>
								<Text variant="pSubtitleSmall/normal" color="gray-60">
									{validatorMap[tooltipData].ipInfo?.city},{' '}
									{validatorMap[tooltipData].ipInfo?.country}
								</Text>
							</div>
						)}
						<div className="my-1 h-px bg-gray-90" />
						<div className="min-w-[120px]">
							<div className="flex justify-between">
								<Text variant="subtitle/medium">Voting Power</Text>
								<Text variant="subtitle/medium">
									{Number(validatorMap[tooltipData].votingPower) / 100}%
								</Text>
							</div>
						</div>
					</TooltipWithBounds>
				)}
			</div>
		</Card>
	);
}
