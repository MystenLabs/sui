// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { lazy, Suspense } from 'react';

import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { TopValidatorsCard } from '~/components/top-validators-card/TopValidatorsCard';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { TableHeader } from '~/ui/TableHeader';
import { Text } from '~/ui/Text';

const ValidatorMap = lazy(
    () => import('../../components/validator-map/ValidatorMap')
);

function ValidatorPageResult() {
    return (
        <div>
            <Heading as="h1" variant="heading2/bold">
                Validators
            </Heading>
        
            <div className="mt-8 flex flex-col md:flex-row gap-5 w-full">
                <div className="grid grid-flow-row bg-gray-40 h-full rounded-sm p-5 md:p-7.5 basis-full md:basis-1/2">
                <div className="flex flex-col flex-nowrap max-w-full p-3.5 gap-1.5 flex-1">
                    <div className="flex gap-0.5 items-baseline">
                        <Text variant="captionSmall/semibold" color="gray-90">Participation</Text>
                        <Text variant="captionSmall/semibold" color="steel-dark">%</Text>
                    </div>
                <div>
                <Heading as="h3" variant="heading2/semibold" color="steel-darker">99.7%</Heading>
            </div>                    
        </div>
                </div>
             <div className="basis-full md:basis-1/2">
                <ErrorBoundary>
                    <Suspense fallback={null}>
                        <ValidatorMap />
                    </Suspense>
                </ErrorBoundary>
            </div>
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
