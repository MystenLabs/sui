// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import type { ReactNode } from 'react';

export type ExternalLinkProps = {
    href: string;
    className?: string;
    children: ReactNode;
    title?: string;
    onClick?(): void;
};

function ExternalLink({
    href,
    className,
    children,
    title,
    onClick,
}: ExternalLinkProps) {
    return (
        <a
            href={href}
            target="_blank"
            className={className}
            rel="noreferrer"
            title={title}
            onClick={onClick}
        >
            {children}
        </a>
    );
}

export default memo(ExternalLink);
