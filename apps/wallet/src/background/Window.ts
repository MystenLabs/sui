// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { filter, fromEventPattern, share, take, takeWhile } from 'rxjs';
import Browser from 'webextension-polyfill';

import { POPUP_HEIGHT, POPUP_WIDTH } from '_src/ui/app/wallet/constants';

const windowRemovedStream = fromEventPattern<number>(
    (handler) => Browser.windows.onRemoved.addListener(handler),
    (handler) => Browser.windows.onRemoved.removeListener(handler)
).pipe(share());

// This is arbitrary across different operating systems, and unfortunately
// there isn't a great way to tell how much extra height we need to tack on
const windowHeightWithFrame = POPUP_HEIGHT + 28;

export class Window {
    private _id: number | null = null;
    private _url: string;

    constructor(url: string) {
        this._url = url;
    }

    public async show() {
        const {
            width = 0,
            left = 0,
            top = 0,
        } = await Browser.windows.getLastFocused();
        const w = await Browser.windows.create({
            url: this._url,
            focused: true,
            width: POPUP_WIDTH,
            height: windowHeightWithFrame,
            type: 'popup',
            top: top,
            left: Math.floor(left + width - 450),
        });
        this._id = typeof w.id === 'undefined' ? null : w.id;
        return windowRemovedStream.pipe(
            takeWhile(() => this._id !== null),
            filter((aWindowID) => aWindowID === this._id),
            take(1)
        );
    }

    public async close() {
        if (this._id !== null) {
            await Browser.windows.remove(this._id);
        }
    }

    /**
     * The id of the window.
     * {@link Window.show} has to be called first. Otherwise this will be null
     * */
    public get id(): number | null {
        return this._id;
    }
}
