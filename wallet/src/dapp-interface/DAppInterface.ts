// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export class DAppInterface {
    private _window: Window;

    constructor(theWindow: Window) {
        this._window = window;
    }
}
