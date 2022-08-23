// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { IS_STATIC_ENV } from './envUtil';

const SEARCH_CATEGORIES = ['transaction', 'object', 'address'];

const deduplicate = (results: [number, string][] | undefined) =>
    results
        ? results
              .map((result) => result[1])
              .filter((value, index, self) => self.indexOf(value) === index)
        : [];

let navigateWithCategory: Function;
let overrideTypeChecks = false;

if (IS_STATIC_ENV) {
    import('./static/searchUtil').then((uf) => {
        navigateWithCategory = uf.navigateWithCategory;
        overrideTypeChecks = true;
    });
} else {
    import('./api/searchUtil').then((uf) => {
        navigateWithCategory = uf.navigateWithCategory;
    });
}

export {
    overrideTypeChecks,
    deduplicate,
    navigateWithCategory,
    SEARCH_CATEGORIES,
};
