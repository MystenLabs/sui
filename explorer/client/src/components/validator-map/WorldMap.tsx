// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Mercator } from '@visx/geo';
import React, { memo } from 'react';
import * as topojson from 'topojson-client';

import { MapFeature } from './MapFeature';
import { NodesLocation } from './NodesLocation';
import world from './topology.json';
import { type Feature, type NodeLocation } from './types';

// @ts-expect-error: The types of `world` here aren't aligned but they are correct
const land = topojson.feature(world, world.objects.countries) as unknown as {
    type: 'FeatureCollection';
    features: Feature[];
};

// We hide Antarctica because there will not be nodes there:
const HIDDEN_REGIONS = ['Antarctica'];
const filteredLand = land.features.filter(
    (feature) => !HIDDEN_REGIONS.includes(feature.properties.name)
);

interface Props {
    width: number;
    height: number;
    nodes?: NodeLocation[];
    onMouseOver(event: React.MouseEvent, countryCode?: string): void;
    onMouseOut(): void;
}

function BaseWorldMap({
    onMouseOver,
    onMouseOut,
    width,
    height,
    nodes,
}: Props) {
    const centerX = width / 2;
    const centerY = height / 2;

    return (
        <svg width={width} height={height}>
            <Mercator
                data={filteredLand}
                scale={100}
                translate={[centerX, centerY + 20]}
            >
                {({ features, projection }) => (
                    <g>
                        <g>
                            {features.map(({ feature, path }, index) => (
                                <MapFeature
                                    key={index}
                                    onMouseOut={onMouseOut}
                                    onMouseOver={onMouseOver}
                                    feature={feature}
                                    path={path}
                                />
                            ))}
                        </g>

                        {nodes?.map((node, index) => (
                            <NodesLocation
                                key={index}
                                node={node}
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
