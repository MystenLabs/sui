// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef, useCallback, useMemo } from 'react';
import {
	// eslint-disable-next-line no-restricted-imports
	Link,
	useHref,
	useLocation,
	// eslint-disable-next-line no-restricted-imports
	useSearchParams,
	// eslint-disable-next-line no-restricted-imports
	useNavigate,
	type NavigateOptions,
	type LinkProps,
} from 'react-router-dom';

export { LinkProps };

/** Query params that we want to be preserved between all pages. */
export const PRESERVE_QUERY = ['network'];

export function useNavigateWithQuery() {
	const navigate = useNavigate();
	const { search } = useLocation();

	const navigateWithQuery = useCallback(
		(url: string, options: NavigateOptions) => navigate(`${url}${search}`, options),
		[navigate, search],
	);

	return navigateWithQuery;
}

export function useSearchParamsMerged() {
	const [searchParams, setSearchParams] = useSearchParams();

	const setSearchParamsMerged = useCallback(
		(params: Record<string, string>, navigateOptions?: Parameters<typeof setSearchParams>[1]) => {
			const nextParams = new URLSearchParams(params);
			PRESERVE_QUERY.forEach((param) => {
				if (searchParams.has(param)) {
					nextParams.set(param, searchParams.get(param)!);
				}
			});
			setSearchParams(nextParams, navigateOptions);
		},
		[searchParams, setSearchParams],
	);

	return [searchParams, setSearchParamsMerged] as const;
}

export const LinkWithQuery = forwardRef<HTMLAnchorElement, LinkProps>(({ to, ...props }, ref) => {
	const href = useHref(to);
	const [searchParams] = useSearchParams();
	const [toBaseURL, toSearchParamString] = href.split('?');

	const mergedSearchParams = useMemo(() => {
		const nextParams = new URLSearchParams(toSearchParamString);
		PRESERVE_QUERY.forEach((param) => {
			if (searchParams.has(param)) {
				nextParams.set(param, searchParams.get(param)!);
			}
		});
		return nextParams.toString();
	}, [toSearchParamString, searchParams]);

	return (
		<Link
			ref={ref}
			to={{
				pathname: toBaseURL,
				search: mergedSearchParams,
			}}
			{...props}
		/>
	);
});
