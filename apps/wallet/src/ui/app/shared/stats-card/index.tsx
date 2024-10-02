// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'clsx';
import { memo } from 'react';
import type { ReactNode } from 'react';

import st from './StatsCard.module.scss';

export type StatsCardProps = {
	className?: string;
	children?: ReactNode | ReactNode[];
};

function StatsCard({ className, children }: StatsCardProps) {
	return <div className={cl(st.container, className)}>{children}</div>;
}

export default memo(StatsCard);
export { default as StatsRow } from './stats-row';
export { default as StatsItem } from './stats-item';
