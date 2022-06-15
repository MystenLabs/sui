// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import BsIcon from '_components/bs-icon';

import type { ReactNode } from 'react';

export type ExternalLinkProps = {
    href: string;
    className?: string;
    children: ReactNode;
    title?: string;
    showIcon?: boolean;
};

function ExternalLink({
    href,
    className,
    children,
    title,
    showIcon = true,
}: ExternalLinkProps) {
    return (
        <a
            href={href}
            target="_blank"
            className={className}
            rel="noreferrer"
            title={title}
        >
            {children} {showIcon ? <BsIcon icon="link-45deg" /> : null}
        </a>
    );
}

export default memo(ExternalLink);
