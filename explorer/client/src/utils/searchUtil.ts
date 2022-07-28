// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { IS_STATIC_ENV } from './envUtil';

const deduplicate = (results: [number, string][] | undefined) =>
    results
        ? results
              .map((result) => result[1])
              .filter((value, index, self) => self.indexOf(value) === index)
        : [];

let navigateWithUnknown: Function;
let overrideTypeChecks = false;

if (IS_STATIC_ENV) {
    import('./static/searchUtil').then((uf) => {
        navigateWithUnknown = uf.navigateWithUnknown;
        overrideTypeChecks = true;
    });
} else {
    import('./api/searchUtil').then(
        (uf) => (navigateWithUnknown = uf.navigateWithUnknown)
    );
}

export { navigateWithUnknown, overrideTypeChecks, deduplicate };
