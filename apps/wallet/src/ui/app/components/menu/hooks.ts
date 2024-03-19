// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createContext, useContext, useMemo } from 'react';
import { useLocation, useSearchParams } from 'react-router-dom';
import type { Location } from 'react-router-dom';

const MENU_PARAM = 'menu';

export const MainLocationContext = createContext<Location | null>(null);

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
	const mainLocationContext = useContext(MainLocationContext);
	const location = useLocation();
	// here we assume that if MainLocationContext is not defined
	// we are not within the menu routes and location is the main one.
	// if it's defined then we use that because useLocation returns the
	// location from the menu Routes that is not what we need here.
	const finalLocation = mainLocationContext || location;
	const { pathname, search } = finalLocation;
	const searchParams = useMemo(() => new URLSearchParams(search.replace('?', '')), [search]);
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
