// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Mercator } from '@visx/geo';
import React, { memo } from 'react';
import * as topojson from 'topojson-client';

import { MapFeature } from './MapFeature';
import { ValidatorLocation } from './ValidatorLocation';
import world from './topology.json';
import { type Feature, type ValidatorMapValidator } from './types';

// @ts-expect-error: The types of `world` here aren't aligned but they are correct
const land = topojson.feature(world, world.objects.countries) as unknown as {
	type: 'FeatureCollection';
	features: Feature[];
};

// We hide Antarctica because there will not be validators there:
const HIDDEN_REGIONS = ['Antarctica'];
const filteredLand = land.features.filter(
	(feature) => !HIDDEN_REGIONS.includes(feature.properties.name),
);

interface Props {
	width: number;
	height: number;
	validators?: ValidatorMapValidator[];
	onMouseOver(event: React.MouseEvent, countryCode?: string): void;
	onMouseOut(): void;
}

function BaseWorldMap({ onMouseOver, onMouseOut, width, height, validators }: Props) {
	const centerX = width / 2;
	const centerY = height / 2;

	return (
		<svg width={width} height={height}>
			<Mercator data={filteredLand} scale={105} translate={[centerX, centerY + 80]}>
				{({ features, projection }) => (
					<g>
						<g>
							{features.map(({ path }, index) => (
								<MapFeature key={index} path={path} />
							))}
						</g>

						{/* Validators need to be sorted by voting power to render smallest nodes on top */}
						{validators
							?.sort((a, b) => {
								const aVal = parseInt(a.votingPower);
								const bVal = parseInt(b.votingPower);
								if (aVal > bVal) return -1;
								else if (aVal < bVal) {
									return 1;
								}
								return 0;
							})
							.map((validator, index) => (
								<ValidatorLocation
									onMouseOut={onMouseOut}
									onMouseOver={onMouseOver}
									key={index}
									validator={validator}
									projection={projection}
								/>
							))}
					</g>
				)}
			</Mercator>
		</svg>
	);
}

// NOTE: Rendering the map is relatively expensive, so we memo this component to improve performance:
export const WorldMap = memo(BaseWorldMap);
