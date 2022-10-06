// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';
import { useLocation, useSearchParams } from 'react-router-dom';

const MENU_PARAM = 'menu';

export function useMenuUrl() {
    const [searchParams] = useSearchParams();
    if (searchParams.has(MENU_PARAM)) {
        return searchParams.get(MENU_PARAM) || '/';
    }
    return false;
}

export function useMenuIsOpen() {
    const [searchParams] = useSearchParams();
    return searchParams.has(MENU_PARAM);
}

/**
 * Get the URL that contains the background page and the menu location
 *
 * @param isOpen Indicates if the menu will be open
 * @param nextMenuLocation The location within the menu
 */
export function useNextMenuUrl(isOpen: boolean, nextMenuLocation = '/') {
    const [searchParams] = useSearchParams();
    const { pathname } = useLocation();
    return useMemo(() => {
        if (isOpen) {
            searchParams.set(MENU_PARAM, nextMenuLocation);
        } else {
            searchParams.delete(MENU_PARAM);
        }
        const search = searchParams.toString();
        return `${pathname}${search ? '?' : ''}${search}`;
    }, [isOpen, nextMenuLocation, searchParams, pathname]);
}
