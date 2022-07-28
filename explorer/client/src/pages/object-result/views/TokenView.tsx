import ReactJson from 'react-json-view';

import DisplayBox from '../../../components/displaybox/DisplayBox';
import Longtext from '../../../components/longtext/Longtext';
import OwnedObjects from '../../../components/ownedobjects/OwnedObjects';
import TxForID from '../../../components/transactions-for-id/TxForID';
import theme from '../../../styles/theme.module.css';
import { getOwnerStr, parseImageURL } from '../../../utils/objectUtils';
import { trimStdLibPrefix } from '../../../utils/stringUtils';
import { type DataType } from '../ObjectResultType';

import styles from './ObjectView.module.css';
function TokenView({ data }: { data: DataType }) {
    const viewedData = {
        ...data,
        objType: trimStdLibPrefix(data.objType),
        name: data.name,
        tx_digest: data.data.tx_digest,
        owner: getOwnerStr(data.owner),
        url: parseImageURL(data.data.contents),
    };

    const detailsTitle = 'Properties';

    const checkIsPropertyType = (value: any) =>
        ['number', 'string'].includes(typeof value);
    const stdLibRe = /0x2::/;
    const prepObjTypeValue = (typeString: string) =>
        typeString.replace(stdLibRe, '');

    const properties = Object.entries(viewedData.data?.contents)
        .filter(([key, _]) => key !== 'name')
        .filter(([_, value]) => checkIsPropertyType(value));

    const structProperties = Object.entries(viewedData.data?.contents)
        .filter(([_, value]) => typeof value == 'object')
        .filter(([key, _]) => key !== 'id');

    let structPropertiesDisplay: any[] = [];
    if (structProperties.length > 0) {
        structPropertiesDisplay = Object.values(structProperties);
    }

    return (
        <div className={styles.resultbox}>
            {viewedData.url !== '' && (
                <div className={styles.display}>
                    <DisplayBox display={viewedData.url} />
                </div>
            )}
            <div
                className={`${styles.textbox} ${
                    viewedData.url ? styles.accommodate : styles.noaccommodate
                }`}
            >
                {viewedData.name && <h1>{viewedData.name}</h1>}{' '}
                <h2 className={styles.header}>Description</h2>
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
                    {viewedData.tx_digest && (
                        <div>
                            <div>Last Transaction ID</div>
                            <div id="lasttxID">
                                <Longtext
                                    text={viewedData.tx_digest}
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
                    {viewedData.publisherAddress && (
                        <div>
                            <div>Publisher</div>
                            <div id="lasttxID">
                                <Longtext
                                    text={viewedData.publisherAddress}
                                    category="addresses"
                                    isLink={true}
                                />
                            </div>
                        </div>
                    )}
                    {viewedData.readonly && (
                        <div>
                            <div>Read Only?</div>
                            {viewedData.readonly === 'true' ? (
                                <div
                                    id="readOnlyStatus"
                                    className={styles.immutable}
                                >
                                    True
                                </div>
                            ) : (
                                <div
                                    id="readOnlyStatus"
                                    className={styles.mutable}
                                >
                                    False
                                </div>
                            )}
                        </div>
                    )}
                    <div>
                        <div>Type</div>
                        <div>{prepObjTypeValue(viewedData.objType)}</div>
                    </div>
                    <div>
                        <div>Owner</div>
                        <div id="owner">
                            <Longtext
                                text={
                                    typeof viewedData.owner === 'string'
                                        ? viewedData.owner
                                        : typeof viewedData.owner
                                }
                                category="unknown"
                                isLink={
                                    viewedData.owner !== 'Immutable' &&
                                    viewedData.owner !== 'Shared'
                                }
                            />
                        </div>
                    </div>
                    {viewedData.contract_id && (
                        <div>
                            <div>Contract ID</div>
                            <Longtext
                                text={viewedData.contract_id.bytes}
                                category="objects"
                                isLink={true}
                            />
                        </div>
                    )}
                </div>
                {properties.length > 0 && (
                    <>
                        <h2 className={styles.header}>{detailsTitle}</h2>
                        <div className={styles.propertybox}>
                            {properties.map(([key, value], index) => (
                                <div key={`property-${index}`}>
                                    <p>{key}</p>
                                    <p>{value}</p>
                                </div>
                            ))}
                        </div>
                    </>
                )}
                {structProperties.length > 0 &&
                    structPropertiesDisplay.map((itm, index) => (
                        <div key={index}>
                            <div className={styles.propertybox}>
                                <div>
                                    <p>{itm[0]}</p>
                                </div>
                            </div>
                            <div className={styles.jsondata}>
                                <div>
                                    <ReactJson
                                        src={itm[1]}
                                        collapsed={2}
                                        name={false}
                                    />
                                </div>
                            </div>
                        </div>
                    ))}
                <h2 className={styles.header}>Child Objects</h2>
                <OwnedObjects id={viewedData.id} byAddress={false} />
                <h2 className={styles.header}>Transactions </h2>
                <TxForID id={viewedData.id} category="object" />
            </div>
        </div>
    );
}

export default TokenView;
