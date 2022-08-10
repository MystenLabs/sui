// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import Longtext from '../../../components/longtext/Longtext';
import ModulesWrapper from '../../../components/module/ModulesWrapper';
import Tabs from '../../../components/tabs/Tabs';
import TxForID from '../../../components/transactions-for-id/TxForID';
import { getOwnerStr } from '../../../utils/objectUtils';
import { trimStdLibPrefix } from '../../../utils/stringUtils';
import { type DataType } from '../ObjectResultType';

import styles from './ObjectView.module.css';

function PkgView({ data }: { data: DataType }) {
    const viewedData = {
        ...data,
        objType: trimStdLibPrefix(data.objType),
        tx_digest: data.data.tx_digest,
        owner: getOwnerStr(data.owner),
    };

    const isPublisherGenesis =
        viewedData.objType === 'Move Package' &&
        viewedData?.publisherAddress === 'Genesis';

    const checkIsPropertyType = (value: any) =>
        ['number', 'string'].includes(typeof value);

    const properties = Object.entries(viewedData.data?.contents)
        .filter(([key, _]) => key !== 'name')
        .filter(([_, value]) => checkIsPropertyType(value));

    const defaultactivetab = 0;

    return (
        <div>
            <div>
                <Tabs selected={defaultactivetab}>
                    <table
                        title="Details"
                        className={styles.description}
                        id="descriptionResults"
                    >
                        <tbody>
                            <tr>
                                <td>Object ID</td>
                                <td id="objectID" className={styles.objectid}>
                                    <Longtext
                                        text={viewedData.id}
                                        category="objects"
                                        isLink={false}
                                        isCopyButton={false}
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
                                        <Longtext
                                            text={viewedData.publisherAddress}
                                            category="addresses"
                                            isLink={!isPublisherGenesis}
                                            isCopyButton={false}
                                        />
                                    </td>
                                </tr>
                            )}
                        </tbody>
                    </table>
                </Tabs>
                <ModulesWrapper
                    data={{
                        title: 'Modules',
                        content: properties,
                    }}
                />
                <h2 className={styles.header}>Transactions </h2>
                <TxForID id={viewedData.id} category="object" />
            </div>
        </div>
    );
}

export default PkgView;
