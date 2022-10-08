// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cn from 'classnames';

import type { ReactNode } from 'react';

import st from './CardLayout.module.scss';

export type CardLayoutProps = {
    children: ReactNode | ReactNode[];
    className?: string;
};

export default function CardLayout({ className, children }: CardLayoutProps) {
    return <div className={cn(className, st.container)}>{children}</div>;
}
