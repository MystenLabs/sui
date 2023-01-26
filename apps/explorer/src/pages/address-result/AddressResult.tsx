// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import OwnedObjects from '../../components/ownedobjects/OwnedObjects';
import TxForID from '../../components/transaction-card/TxForID';

import { Banner } from '~/ui/Banner';
import { PageHeader } from '~/ui/PageHeader';

type DataType = {
    id: string;
    objects: ResponseType;
    loadState?: 'loaded' | 'pending' | 'fail';
};

type ResponseType = {
    objectId: string;
}[];

function instanceOfDataType(object: any): object is DataType {
    return object !== undefined && ['id', 'objects'].every((x) => x in object);
}

function AddressResult() {
    const { id: addressID } = useParams();

    if (addressID !== undefined) {
        return (
            <>
                <div className="mt-5 mb-10">
                    <PageHeader type="Address" title={addressID} />
                </div>
                <div>
                    <h1>Owned Objects</h1>
                    <div>
                        <ErrorBoundary>
                            <OwnedObjects id={addressID} byAddress />
                        </ErrorBoundary>
                    </div>
                </div>
                <div>
                    <h1>Transactions</h1>
                    <ErrorBoundary>
                        <TxForID id={addressID} category="address" />
                    </ErrorBoundary>
                </div>
            </>
        );
    } else {
        return (
            <Banner variant="error" spacing="lg" fullWidth>
                Something went wrong
            </Banner>
        );
    }
}

export default AddressResult;
export { instanceOfDataType };
