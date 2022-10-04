// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBook } from '@growthbook/growthbook';

export default class FeatureGating {
    private _growthBook: GrowthBook;
    constructor() {
        // Create a GrowthBook context
        this._growthBook = new GrowthBook();
    }

    public async init() {
        if (process.env.NODE_ENV !== 'production') {
            return;
        }
        // Load feature definitions
        await fetch(
            `https://cdn.growthbook.io/api/features/${process.env.GROWTH_BOOK_API_KEY}`
        )
            .then((res) => {
                if (res.ok) {
                    return res.json();
                }
                throw new Error(res.statusText);
            })
            .then((parsed) => {
                this._growthBook.setFeatures(parsed.features);
            })
            .catch(() => {
                console.warn(
                    `Failed to fetch feature definitions from GrowthBook with API_KEY ${process.env.GROWTH_BOOK_API_KEY}`
                );
            });
    }

    public isOn(featureName: string): boolean {
        return this._growthBook.isOn(featureName);
    }
}
