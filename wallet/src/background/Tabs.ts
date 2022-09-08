// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromEventPattern, map, mergeWith, share, Subject } from 'rxjs';
import Browser from 'webextension-polyfill';

import type { Tabs as BrowserTabs } from 'webextension-polyfill';

const onRemovedStream = fromEventPattern<
    [number, BrowserTabs.OnRemovedRemoveInfoType]
>(
    (handler) => Browser.tabs.onRemoved.addListener(handler),
    (handler) => Browser.tabs.onRemoved.removeListener(handler)
).pipe(share());

const onCreatedStream = fromEventPattern<BrowserTabs.Tab>(
    (handler) => Browser.tabs.onCreated.addListener(handler),
    (handler) => Browser.tabs.onCreated.removeListener(handler)
).pipe(share());

const onUpdatedStream = fromEventPattern<
    [number, BrowserTabs.OnUpdatedChangeInfoType, BrowserTabs.Tab]
>(
    (handler) => Browser.tabs.onUpdated.addListener(handler),
    (handler) => Browser.tabs.onUpdated.removeListener(handler)
).pipe(share());

type TabInfo = {
    id: number;
    url: string | null;
    closed?: boolean;
};

class Tabs {
    private tabs: Map<number, TabInfo> = new Map();
    private _onRemoved: Subject<TabInfo> = new Subject();

    constructor() {
        Browser.tabs.query({}).then((tabs) => {
            for (const { id, url } of tabs) {
                if (id && url) {
                    this.tabs.set(id, { id, url });
                }
            }
        });
        onCreatedStream
            .pipe(
                mergeWith(onUpdatedStream.pipe(map(([_1, _2, aTab]) => aTab)))
            )
            .subscribe((aTab) => {
                const { id, url } = aTab;
                if (id && url) {
                    const currentTab = this.tabs.get(id);
                    if (
                        currentTab &&
                        currentTab.url &&
                        currentTab.url !== url
                    ) {
                        // notify as removed tab when we change the url
                        this._onRemoved.next({
                            id,
                            url: currentTab.url,
                            closed: false,
                        });
                    }
                    this.tabs.set(id, { id, url });
                }
            });
        onRemovedStream.subscribe(([tabID]) => {
            const tabInfo: TabInfo = this.tabs.get(tabID) || {
                id: tabID,
                url: null,
            };
            tabInfo.closed = true;
            this.tabs.delete(tabID);
            this._onRemoved.next(tabInfo);
        });
    }

    /**
     * An observable that emits when a tab wea closed or when the url has changed
     */
    public get onRemoved() {
        return this._onRemoved.asObservable();
    }

    public async highlight(option: { url: string } | { tabID: number }) {
        let tabToHighlight: BrowserTabs.Tab | null = null;
        if ('tabID' in option) {
            try {
                tabToHighlight = await Browser.tabs.get(option.tabID);
            } catch (e) {
                //Do nothing
            }
        }
        if ('url' in option) {
            const url = option.url.split('#')[0];
            const tabs = (
                await Browser.tabs.query({
                    url,
                })
            ).filter((aTab) => aTab.url === option.url);
            if (tabs.length) {
                tabToHighlight = tabs[0];
            }
        }
        if (!tabToHighlight) {
            return false;
        }
        if (tabToHighlight.windowId) {
            await Browser.windows.update(tabToHighlight.windowId, {
                drawAttention: true,
                focused: true,
            });
        }
        await Browser.tabs.highlight({
            tabs: tabToHighlight.index,
            windowId: tabToHighlight.windowId,
        });
        return true;
    }
}

export default new Tabs();
