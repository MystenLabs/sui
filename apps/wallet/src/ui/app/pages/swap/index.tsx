// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Route, Routes } from 'react-router-dom';

import { Assets } from './Assets';
import { SwapPageForm } from './SwapPageForm';
import { ErrorBoundary } from '_components/error-boundary';

export function SwapPage() {
	return (
		<ErrorBoundary>
			<Routes>
				<Route path="/" element={<SwapPageForm />} />
				<Route path="/base-assets" element={<Assets />} />
			</Routes>
		</ErrorBoundary>
	);
}
