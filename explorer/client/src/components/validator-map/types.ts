// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export interface Feature {
    type: 'Feature';
    id: string;
    geometry: { coordinates: [number, number][][]; type: 'Polygon' };
    properties: { name: string; alpha2: string };
}

export interface NodeLocation {
    count: number;
    city: string;
    countryCode: string;
    location: [lat: number, long: number];
}
