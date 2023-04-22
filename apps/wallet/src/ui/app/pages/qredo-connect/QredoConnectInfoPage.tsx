// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useParams } from 'react-router-dom';

import { Button } from '../../shared/ButtonUI';
import { PageMainLayoutTitle } from '../../shared/page-main-layout/PageMainLayoutTitle';
import { Text } from '../../shared/text';

export function QredoConnectInfoPage() {
    const { requestID } = useParams();
    // eslint-disable-next-line no-console
    console.log('QredoConnectInfoPage', requestID);
    return (
        <>
            <PageMainLayoutTitle title="Qredo Accounts Setup" />
            <div className="flex flex-col flex-nowrap gap-10 justify-center flex-1 p-6 items-center">
                <Text>Qredo connect is under construction.</Text>
                <Button
                    variant="secondary"
                    text="Close"
                    onClick={() => window.close()}
                />
            </div>
        </>
    );
}
