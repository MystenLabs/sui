// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import { ErrorBoundary } from '../../components/error-boundary/ErrorBoundary';
import { useGetObject } from '../../hooks/useGetObject';
import { extractName } from '../../utils/objectUtils';
import { translate, type DataType } from './ObjectResultType';
import PkgView from './views/PkgView';
import TokenView from './views/TokenView';

import { Banner } from '~/ui/Banner';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { PageHeader } from '~/ui/PageHeader';

const PACKAGE_TYPE_NAME = 'Move Package';

function Fail({ objID }: { objID: string | undefined }) {
    return (
        <Banner variant="error" spacing="lg" fullWidth>
            Data could not be extracted on the following specified object ID:{' '}
            {objID}
        </Banner>
    );
}

export function ObjectResult() {
    const { id: objID } = useParams();
    const { data, isLoading, isError } = useGetObject(objID!);

    if (isLoading) {
        return <LoadingSpinner text="Loading data" />;
    }

    if (isError) {
        return <Fail objID={objID} />;
    }

    // TODO: Handle status better NotExists, Deleted, Other
    if (data?.status !== 'Exists') {
        return <Fail objID={objID} />;
    }

    const resp = translate(data);
    const name = extractName(resp.data?.contents);
    const isPackage = resp.objType === PACKAGE_TYPE_NAME;

    return (
        <div className="mt-5 mb-10">
            <PageHeader
                type={isPackage ? 'Package' : 'Object'}
                title={resp.id}
                subtitle={name}
            />

            <ErrorBoundary>
                <div className="mt-10">
                    {isPackage ? (
                        <PkgView data={resp} />
                    ) : (
                        <TokenView data={resp} />
                    )}
                </div>
            </ErrorBoundary>
        </div>
    );
}

export type { DataType };
