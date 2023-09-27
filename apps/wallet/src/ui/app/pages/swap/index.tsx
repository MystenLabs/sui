// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '_components/error-boundary';
import { QuoteAssets } from '_pages/swap/QuoteAssets';
import { Route, Routes } from 'react-router-dom';

import { BaseAssets } from './BaseAssets';
import { SwapPageForm } from './SwapPageForm';

export function SwapPage() {
	return (
		<ErrorBoundary>
			<Routes>
				<Route path="/" element={<SwapPageForm />} />
				<Route path="/base-assets" element={<BaseAssets />} />
				<Route path="/quote-assets" element={<QuoteAssets />} />
			</Routes>
		</ErrorBoundary>
	);
}
