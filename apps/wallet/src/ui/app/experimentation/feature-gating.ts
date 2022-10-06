// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBook } from '@growthbook/growthbook';

const GROWTHBOOK_API_KEY =
    process.env.GROWTH_BOOK_API_KEY ?? 'key_dev_dc2872e15e0c5f95';

export default class FeatureGating {
    #growthBook: GrowthBook;

    constructor() {
        // Create a GrowthBook context
        this.#growthBook = new GrowthBook();
    }

    public async init() {
        // Load feature definitions
        await fetch(
            `https://cdn.growthbook.io/api/features/${GROWTHBOOK_API_KEY}`
        )
            .then((res) => {
                if (res.ok) {
                    return res.json();
                }
                throw new Error(res.statusText);
            })
            .then((parsed) => {
                this.#growthBook.setFeatures(parsed.features);
            })
            .catch(() => {
                // eslint-disable-next-line no-console
                console.warn(
                    `Failed to fetch feature definitions from GrowthBook with API_KEY ${process.env.GROWTH_BOOK_API_KEY}`
                );
            });
    }

    public isOn(featureName: string): boolean {
        return this.#growthBook.isOn(featureName);
    }
}
