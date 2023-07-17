// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Activity } from '../../components/Activity';
import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { PageLayout } from '~/components/Layout/PageLayout';
import { useSearchParamsMerged } from '~/ui/utils/LinkWithQuery';

const TRANSACTIONS_LIMIT = 20;

export function Recent() {
	const [searchParams] = useSearchParamsMerged();

	return (
		<PageLayout
			content={
				<div data-testid="transaction-page" id="transaction" className="mx-auto">
					<ErrorBoundary>
						<Activity initialLimit={TRANSACTIONS_LIMIT} initialTab={searchParams.get('tab')} />
					</ErrorBoundary>
				</div>
			}
		/>
	);
}
