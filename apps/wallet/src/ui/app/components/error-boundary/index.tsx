// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Alert from '_components/alert';
import type { ReactNode } from 'react';
import { ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import type { FallbackProps } from 'react-error-boundary';
import { useLocation } from 'react-router-dom';

function Fallback({ error }: FallbackProps) {
	return (
		<div className="p-2">
			<Alert>
				<div className="mb-1 font-semibold">Something went wrong</div>
				<div className="font-mono">{error.message}</div>
			</Alert>
		</div>
	);
}

export type ErrorBoundaryProps = {
	children: ReactNode | ReactNode[];
};

export function ErrorBoundary({ children }: ErrorBoundaryProps) {
	const location = useLocation();
	return (
		<ReactErrorBoundary FallbackComponent={Fallback} resetKeys={[location]}>
			{children}
		</ReactErrorBoundary>
	);
}
