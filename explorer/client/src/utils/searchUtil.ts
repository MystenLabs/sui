// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

let navigateWithUnknown: Function;

if (process.env.REACT_APP_DATA === 'static') {
    import('./static/searchUtil').then(
        (uf) => (navigateWithUnknown = uf.navigateWithUnknown)
    );
} else {
    import('./api/searchUtil').then(
        (uf) => (navigateWithUnknown = uf.navigateWithUnknown)
    );
}

export { navigateWithUnknown };
