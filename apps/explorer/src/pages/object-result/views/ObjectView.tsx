// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '../../../components/error-boundary/ErrorBoundary';
import { extractName } from '../../../utils/objectUtils';
import { type DataType } from '../ObjectResultType';
import PkgView from './PkgView';
import TokenView from './TokenView';

import { PageHeader } from '~/ui/PageHeader';

const PACKAGE_TYPE_NAME = 'Move Package';

function ObjectView({ data }: { data: DataType }) {
    const name = extractName(data.data?.contents);

    const isPackage = data.objType === PACKAGE_TYPE_NAME;

    return (
        <>
            <div className="mt-5 mb-10">
                <PageHeader
                    type={isPackage ? 'Package' : 'Object'}
                    title={data.id}
                    subtitle={name}
                />
            </div>
            <ErrorBoundary>
                {isPackage ? (
                    <PkgView data={data} />
                ) : (
                    <TokenView data={data} />
                )}
            </ErrorBoundary>
        </>
    );
}

export default ObjectView;
