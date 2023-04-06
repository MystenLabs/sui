// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionSender } from '@mysten/sui.js';

import { ErrorBoundary } from '../../../components/error-boundary/ErrorBoundary';
import PkgModulesWrapper from '../../../components/module/PkgModulesWrapper';
import { TransactionsForAddress } from '../../../components/transactions/TransactionsForAddress';
import { useGetTransaction } from '../../../hooks/useGetTransaction';
import { getOwnerStr } from '../../../utils/objectUtils';
import { trimStdLibPrefix } from '../../../utils/stringUtils';
import { type DataType } from '../ObjectResultType';

import styles from './ObjectView.module.css';

import { Heading } from '~/ui/Heading';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

const GENESIS_TX_DIGEST = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=';

function PkgView({ data }: { data: DataType }) {
    const { data: txnData, isLoading } = useGetTransaction(
        data.data.tx_digest!
    );

    if (isLoading) {
        return <LoadingSpinner text="Loading data" />;
    }
    const viewedData = {
        ...data,
        objType: trimStdLibPrefix(data.objType),
        tx_digest: data.data.tx_digest,
        owner: getOwnerStr(data.owner),
        publisherAddress:
            data.data.tx_digest === GENESIS_TX_DIGEST
                ? 'Genesis'
                : getTransactionSender(txnData!),
    };

    const checkIsPropertyType = (value: any) =>
        ['number', 'string'].includes(typeof value);

    const properties = Object.entries(viewedData.data?.contents)
        .filter(([key, _]) => key !== 'name')
        .filter(([_, value]) => checkIsPropertyType(value));

    return (
        <div>
            <div>
                <TabGroup size="lg">
                    <TabList>
                        <Tab>Details</Tab>
                    </TabList>
                    <TabPanels>
                        <TabPanel>
                            <table
                                className={styles.description}
                                id="descriptionResults"
                            >
                                <tbody>
                                    <tr>
                                        <td>Object ID</td>
                                        <td
                                            id="objectID"
                                            className={styles.objectid}
                                        >
                                            <ObjectLink
                                                objectId={viewedData.id}
                                                noTruncate
                                            />
                                        </td>
                                    </tr>

                                    <tr>
                                        <td>Version</td>
                                        <td>{viewedData.version}</td>
                                    </tr>

                                    {viewedData?.publisherAddress && (
                                        <tr>
                                            <td>Publisher</td>
                                            <td id="lasttxID">
                                                <AddressLink
                                                    address={
                                                        viewedData.publisherAddress
                                                    }
                                                    noTruncate
                                                />
                                            </td>
                                        </tr>
                                    )}
                                </tbody>
                            </table>
                        </TabPanel>
                    </TabPanels>
                </TabGroup>

                <div className="mb-3">
                    <Heading as="h2" variant="heading4/semibold">
                        Modules
                    </Heading>
                </div>
                <ErrorBoundary>
                    <PkgModulesWrapper id={data.id} modules={properties} />
                </ErrorBoundary>
                <div className={styles.txsection}>
                    <ErrorBoundary>
                        <TransactionsForAddress
                            address={viewedData.id}
                            type="object"
                        />
                    </ErrorBoundary>
                </div>
            </div>
        </div>
    );
}

export default PkgView;
