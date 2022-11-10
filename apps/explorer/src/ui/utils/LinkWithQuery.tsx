// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef, useCallback } from 'react';
import {
    // eslint-disable-next-line no-restricted-imports
    Link,
    useLocation,
    // eslint-disable-next-line no-restricted-imports
    useNavigate,
    type NavigateOptions,
    type LinkProps,
} from 'react-router-dom';

export { LinkProps };

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

export const LinkWithQuery = forwardRef<HTMLAnchorElement, LinkProps>(
    ({ to, ...props }, ref) => {
        const { search } = useLocation();

        return <Link ref={ref} to={`${to}${search}`} {...props} />;
    }
);
