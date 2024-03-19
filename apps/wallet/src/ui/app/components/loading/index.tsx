// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';

import LoadingIndicator, { type LoadingIndicatorProps } from './LoadingIndicator';

type LoadingProps = {
	loading: boolean;
	children: ReactNode | ReactNode[];
} & LoadingIndicatorProps;

const Loading = ({ loading, children, ...indicatorProps }: LoadingProps) => {
	return loading ? (
		<div className="flex justify-center items-center h-full">
			<LoadingIndicator {...indicatorProps} />
		</div>
	) : (
		<>{children}</>
	);
};

export default Loading;
