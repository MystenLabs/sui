// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Mercator } from '@visx/geo';
import { forwardRef, memo } from 'react';
import * as topojson from 'topojson-client';
import world from 'world-atlas/countries-50m.json';

interface FeatureShape {
    type: 'Feature';
    id: string;
    geometry: { coordinates: [number, number][][]; type: 'Polygon' };
    properties: { name: string };
}

const land = topojson.feature(world, world.objects.countries) as unknown as {
    type: 'FeatureCollection';
    features: FeatureShape[];
};

// We hide Antarctica because there will not be nodes there:
const HIDDEN_REGIONS = ['Antarctica'];
const filteredLand = land.features.filter(
    (feature) => !HIDDEN_REGIONS.includes(feature.properties.name)
);

export interface NodeLocation {
    count: number;
    city: string;
    country: string;
    location: [lat: number, long: number];
}

interface MapProps {
    width: number;
    height: number;
    nodes?: NodeLocation[];
    onMouseOver(event: React.MouseEvent): void;
    onMouseOut(event: React.MouseEvent): void;
}

const BaseWorldMap = forwardRef<SVGSVGElement, MapProps>(
    ({ onMouseOver, onMouseOut, width, height, nodes }, ref) => {
        const centerX = width / 2;
        const centerY = height / 2;

        return (
            <svg ref={ref} width={width} height={height}>
                <Mercator
                    data={filteredLand}
                    scale={100}
                    translate={[centerX, centerY + 20]}
                >
                    {({ features, projection }) => (
                        <g>
                            <g>
                                {features.map(({ feature, path }, i) => (
                                    <path
                                        key={i}
                                        name={feature.properties.name}
                                        onMouseOver={onMouseOver}
                                        onMouseMove={onMouseOver}
                                        onMouseOut={onMouseOut}
                                        d={path || ''}
                                        fill="white"
                                        // stroke="#F3F4F5"
                                        // stroke={background}
                                        // strokeWidth={0.5}
                                    />
                                ))}
                            </g>

                            {nodes?.map(({ location, city }, index) => {
                                const position = projection(location);

                                if (!position) return null;

                                return (
                                    <circle
                                        style={{ pointerEvents: 'none' }}
                                        key={index}
                                        cx={position[0]}
                                        cy={position[1]}
                                        r={10}
                                        fill="#6FBCF0"
                                        opacity={0.4}
                                        data-name={city}
                                    />
                                );
                            })}
                        </g>
                    )}
                </Mercator>
            </svg>
        );
    }
);

export const WorldMap = memo(BaseWorldMap);
