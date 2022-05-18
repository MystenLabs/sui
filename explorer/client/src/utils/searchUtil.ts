// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

let navigateWithUnknown: Function;
let overrideTypeChecks = false;

if (process.env.REACT_APP_DATA === 'static') {
    import('./static/searchUtil').then((uf) => {
        navigateWithUnknown = uf.navigateWithUnknown;
        overrideTypeChecks = true;
    });
} else {
    import('./api/searchUtil').then(
        (uf) => (navigateWithUnknown = uf.navigateWithUnknown)
    );
}

export { navigateWithUnknown, overrideTypeChecks };
