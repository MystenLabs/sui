// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import LoadingIndicator, { type LoadingIndicatorProps } from './LoadingIndicator';

import type { ReactNode } from 'react';

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
