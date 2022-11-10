// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef } from 'react';
// eslint-disable-next-line no-restricted-imports
import { Link, useLocation, type LinkProps } from 'react-router-dom';

export { LinkProps };

export const LinkWithQuery = forwardRef<HTMLAnchorElement, LinkProps>(
    ({ to, ...props }) => {
        const { search } = useLocation();

        return <Link to={`${to}${search}`} {...props} />;
    }
);
