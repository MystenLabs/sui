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

// TODO: Once we have a new router configuration based on the new react router configuration,
// we should just move these components there so that we import the link from the router.
// This also will align closer to how TanStack Router works.

export function useNavigateWithQuery() {
    const navigate = useNavigate();
    const { search } = useLocation();

    const navigateWithQuery = useCallback(
        (url: string, options: NavigateOptions) => {
            return navigate(`${url}${search}`, options);
        },
        [navigate, search]
    );

    return navigateWithQuery;
}

export function useSearchParamsMerged() {
    const [searchParams, setSearchParams] = useSearchParams();

    const setSearchParamsMerged = useCallback(
        (
            params: Record<string, any>,
            navigateOptions?: Parameters<typeof setSearchParams>[1]
        ) => {
            const nextParams = new URLSearchParams(searchParams);
            Object.entries(params).forEach(([key, value]) => {
                if (typeof value === 'undefined' || value === null) {
                    nextParams.delete(key);
                } else {
                    nextParams.set(key, value);
                }
            });
            setSearchParams(nextParams, navigateOptions);
        },
        [searchParams, setSearchParams]
    );

    return [searchParams, setSearchParamsMerged] as const;
}

export const LinkWithQuery = forwardRef<HTMLAnchorElement, LinkProps>(
    ({ to, ...props }) => {
        const href = useHref(to);
        const [searchParams] = useSearchParams();
        const [toBaseURL, toSearchParamString] = href.split('?');

        const mergedSearchParams = useMemo(() => {
            return new URLSearchParams({
                ...Object.fromEntries(searchParams),
                ...Object.fromEntries(new URLSearchParams(toSearchParamString)),
            }).toString();
        }, [toSearchParamString, searchParams]);

        return (
            <Link
                to={{
                    pathname: toBaseURL,
                    search: mergedSearchParams,
                }}
                {...props}
            />
        );
    }
);
