// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { TopValidatorsCard } from '../../components/top-validators-card/TopValidatorsCard';

import { Heading } from '~/ui/Heading';

function ValidatorPageResult() {
    return (
        <div>
            <Heading as="h1" variant="heading2" weight="bold">
                Validators
            </Heading>
            <div className="mt-8">
                <ErrorBoundary>
                    <TopValidatorsCard />
                </ErrorBoundary>
            </div>
        </div>
    );
}

export { ValidatorPageResult };
