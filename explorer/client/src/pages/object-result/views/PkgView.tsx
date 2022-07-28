import Longtext from '../../../components/longtext/Longtext';
import ModulesWrapper from '../../../components/module/ModulesWrapper';
import TxForID from '../../../components/transactions-for-id/TxForID';
import theme from '../../../styles/theme.module.css';
import { getOwnerStr } from '../../../utils/objectUtils';
import { trimStdLibPrefix } from '../../../utils/stringUtils';
import { type DataType } from '../ObjectResultType';

import styles from './ObjectView.module.css';

function PkgView({ data }: { data: DataType }) {
    const viewedData = {
        ...data,
        objType: trimStdLibPrefix(data.objType),
        name: data.name,
        tx_digest: data.data.tx_digest,
        owner: getOwnerStr(data.owner),
    };

    const descriptionTitle = 'Package Description';
    const isPublisherGenesis =
        viewedData.objType === 'Move Package' &&
        viewedData?.publisherAddress === 'Genesis';

    const checkIsPropertyType = (value: any) =>
        ['number', 'string'].includes(typeof value);

    const properties = Object.entries(viewedData.data?.contents)
        .filter(([key, _]) => key !== 'name')
        .filter(([_, value]) => checkIsPropertyType(value));

    return (
        <div className={styles.resultbox}>
            <div className={`${styles.textbox} ${styles.noaccommodate}`}>
                {viewedData.name && <h1>{viewedData.name}</h1>}{' '}
                <h2 className={styles.header}>{descriptionTitle}</h2>
                <div className={theme.textresults} id="descriptionResults">
                    <div>
                        <div>Object ID</div>
                        <div id="objectID">
                            <Longtext
                                text={viewedData.id}
                                category="objects"
                                isLink={false}
                            />
                        </div>
                    </div>
                    {data.data?.tx_digest && !isPublisherGenesis && (
                        <div>
                            <div>Last Transaction ID</div>
                            <div id="lasttxID">
                                <Longtext
                                    text={data.data?.tx_digest}
                                    category="transactions"
                                    isLink={true}
                                />
                            </div>
                        </div>
                    )}
                    <div>
                        <div>Version</div>
                        <div>{viewedData.version}</div>
                    </div>
                    {viewedData?.publisherAddress && (
                        <div>
                            <div>Publisher</div>
                            <div id="lasttxID">
                                <Longtext
                                    text={viewedData.publisherAddress}
                                    category="addresses"
                                    isLink={!isPublisherGenesis}
                                />
                            </div>
                        </div>
                    )}
                </div>
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
