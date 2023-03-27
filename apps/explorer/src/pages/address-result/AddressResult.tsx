// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import OwnedObjects from '../../components/ownedobjects/OwnedObjects';
import { TransactionsForAddress } from '../../components/transactions/TransactionsForAddress';

import { Heading } from '~/ui/Heading';
import { PageHeader } from '~/ui/PageHeader';

function AddressResult() {
    const { id: addressID } = useParams();

    return (
        <div className="space-y-12">
            <PageHeader type="Address" title={addressID!} />

            <div>
                <div className="border-b border-gray-45 pb-5 md:mt-12">
                    <Heading color="gray-90" variant="heading4/semibold">
                        Owned Objects
                    </Heading>
                </div>
                <ErrorBoundary>
                    <OwnedObjects id={addressID!} byAddress />
                </ErrorBoundary>
            </div>

            <div>
                <div className="border-b border-gray-45 pb-5">
                    <Heading color="gray-90" variant="heading4/semibold">
                        Transaction Blocks
                    </Heading>
                </div>
                <ErrorBoundary>
                    <div className="mt-2">
                        <TransactionsForAddress
                            address={addressID!}
                            type="address"
                        />
                    </div>
                </ErrorBoundary>
            </div>
        </div>
    );
}

export default AddressResult;
