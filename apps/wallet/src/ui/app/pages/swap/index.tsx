// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '_components/error-boundary';
import { Route, Routes } from 'react-router-dom';

import { Assets } from './Assets';
import { SwapPageForm } from './SwapPageForm';

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
