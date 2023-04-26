// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    BehaviorSubject,
    distinctUntilChanged,
    filter,
    from,
    fromEventPattern,
    map,
    merge,
    mergeWith,
    share,
    Subject,
    switchMap,
} from 'rxjs';
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

const onTabActivated = fromEventPattern<BrowserTabs.OnActivatedActiveInfoType>(
    (handler) => Browser.tabs.onActivated.addListener(handler),
    (handler) => Browser.tabs.onActivated.removeListener(handler)
).pipe(share());

const onWindowFocusChanged = fromEventPattern<number>(
    (handler) => Browser.windows.onFocusChanged.addListener(handler),
    (handler) => Browser.windows.onFocusChanged.removeListener(handler)
).pipe(share());

type TabInfo = {
    id: number;
    url: string | null;
    nextUrl?: string;
    closed?: boolean;
};

type ActiveOriginInfo = {
    origin: string | null;
    favIcon: string | null;
};

class Tabs {
    private tabs: Map<number, TabInfo> = new Map();
    private _onRemoved: Subject<TabInfo> = new Subject();
    private _onActiveOrigin: BehaviorSubject<ActiveOriginInfo> =
        new BehaviorSubject<ActiveOriginInfo>({ origin: null, favIcon: null });

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
                            nextUrl: url,
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
        merge(
            onWindowFocusChanged.pipe(
                switchMap((windowId) =>
                    Browser.tabs.query({ active: true, windowId })
                ),
                map((tabs) => tabs[0])
            ),
            from(
                Browser.tabs.query({ active: true, lastFocusedWindow: true })
            ).pipe(map((tabs) => tabs[0])),
            onTabActivated.pipe(
                switchMap((info) =>
                    merge(
                        Browser.tabs.get(info.tabId),
                        onUpdatedStream.pipe(
                            filter(([tabID]) => info.tabId === tabID),
                            map(([_1, _2, tab]) => tab)
                        )
                    )
                )
            )
        )
            .pipe(
                map((tab) => ({
                    origin: tab.url ? new URL(tab.url).origin : null,
                    favIcon: tab.favIconUrl || null,
                })),
                distinctUntilChanged(
                    (prev, current) =>
                        prev.origin === current.origin &&
                        prev.favIcon === current.favIcon
                )
            )
            .subscribe((activeOrigin) => {
                this._onActiveOrigin.next(activeOrigin);
            });
    }

    /**
     * An observable that emits when a tab wea closed or when the url has changed
     */
    public get onRemoved() {
        return this._onRemoved.asObservable();
    }

    public get activeOrigin() {
        return this._onActiveOrigin.asObservable();
    }

    public async highlight(
        option: { windowID?: number } & (
            | {
                  url: string;
                  match?: (values: {
                      url: string;
                      inAppRedirectUrl?: string;
                  }) => boolean;
              }
            | { tabID: number }
        )
    ) {
        let tabToHighlight: BrowserTabs.Tab | null = null;
        if ('tabID' in option) {
            try {
                tabToHighlight = await Browser.tabs.get(option.tabID);
            } catch (e) {
                //Do nothing
            }
        } else {
            const inAppUrlToMatch = option.url.split('#')[1] || '';
            const tabs = (
                await Browser.tabs.query({
                    url: option.url.split('#')[0],
                    windowId: option.windowID,
                })
            ).filter((aTab) => {
                let inAppRedirectUrl: string | undefined = undefined;
                if (aTab.url === option.url) {
                    return true;
                }
                if (!aTab.url) {
                    return false;
                }
                try {
                    const tabURL = new URL(aTab.url);
                    if (tabURL.hash.startsWith('#/locked?url=')) {
                        inAppRedirectUrl = decodeURIComponent(
                            tabURL.hash.replace('#/locked?url=', '')
                        );
                        if (inAppRedirectUrl === inAppUrlToMatch) {
                            return true;
                        }
                    }
                } catch (e) {
                    // do nothing
                }
                if (
                    option.match &&
                    option.match({ url: aTab.url, inAppRedirectUrl })
                ) {
                    return true;
                }
                return false;
            });
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
