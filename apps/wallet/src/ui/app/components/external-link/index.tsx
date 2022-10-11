// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import Icon from '_components/icon';

import type { ReactNode } from 'react';

export type ExternalLinkProps = {
    href: string;
    className?: string;
    children: ReactNode;
    title?: string;
    showIcon?: boolean;
    onClick?(): void;
};

function ExternalLink({
    href,
    className,
    children,
    title,
    showIcon = true,
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
            {showIcon ? <Icon icon="link-45deg" /> : null}
        </a>
    );
}

export default memo(ExternalLink);
