// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { lazy, Suspense } from 'react';

import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { TopValidatorsCard } from '~/components/top-validators-card/TopValidatorsCard';
import { Heading } from '~/ui/Heading';
import { TableHeader } from '~/ui/TableHeader';

const ValidatorMap = lazy(
    () => import('../../components/validator-map/ValidatorMap')
);

function ValidatorPageResult() {
    return (
        <div>
            <Heading as="h1" variant="heading2/bold">
                Validators
            </Heading>
            <div className="mt-8">
                <ErrorBoundary>
                    <Suspense fallback={null}>
                        <ValidatorMap />
                    </Suspense>
                </ErrorBoundary>
            </div>
            <div className="mt-8">
                <ErrorBoundary>
                    <TableHeader>All Validators</TableHeader>
                    <TopValidatorsCard showIcon />
                </ErrorBoundary>
            </div>
        </div>
    );
}

export { ValidatorPageResult };
