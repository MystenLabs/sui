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

interface Node {
    ipAddress: string;
    city: string;
    country: string;
    location: [lat: number, long: number];
}

interface MapProps {
    width: number;
    height: number;
    nodes: Node[];
    onMouseOver(event: React.MouseEvent): void;
    onMouseOut(event: React.MouseEvent): void;
}

const BaseWorldMap = forwardRef<SVGSVGElement, MapProps>(
    ({ onMouseOver, onMouseOut, width, height, nodes }, ref) => {
        const centerX = width / 2;
        const centerY = height / 2;
        const scale = (width / 630) * 100;

        return (
            <svg ref={ref} width={width} height={height}>
                <Mercator
                    data={land.features}
                    scale={scale}
                    translate={[centerX, centerY + 20]}
                >
                    {({ features, projection }) => (
                        <g>
                            {features.map(({ feature, path }, i) => (
                                <path
                                    name={feature.properties.name}
                                    onMouseOver={onMouseOver}
                                    onMouseMove={onMouseOver}
                                    onMouseOut={onMouseOut}
                                    key={`map-feature-${i}`}
                                    d={path || ''}
                                    fill="white"
                                    // stroke="#F3F4F5"
                                    // stroke={background}
                                    // strokeWidth={0.5}
                                    // strokeWidth={0}
                                />
                            ))}

                            {nodes.map(({ location }, index) => {
                                const position = projection(location);

                                if (!position) return null;

                                return (
                                    <circle
                                        key={index}
                                        x={position[0]}
                                        y={position[1]}
                                        width={15}
                                        height={15}
                                        fill="#6FBCF0"
                                        opacity={0.4}
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
