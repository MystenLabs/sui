// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react';

import { ReactComponent as PreviewMediaIcon } from '../../../assets/SVGIcons/preview-media.svg';
import DisplayBox from '../../../components/displaybox/DisplayBox';
import Longtext from '../../../components/longtext/Longtext';
import ModulesWrapper from '../../../components/module/ModulesWrapper';
import OwnedObjects from '../../../components/ownedobjects/OwnedObjects';
import TxForID from '../../../components/transaction-card/TxForID';
import {
    getOwnerStr,
    parseImageURL,
    checkIsPropertyType,
    extractName,
} from '../../../utils/objectUtils';
import {
    trimStdLibPrefix,
    genFileTypeMsg,
    normalizeSuiAddress,
} from '../../../utils/stringUtils';
import { type DataType } from '../ObjectResultType';

import styles from './ObjectView.module.css';

import { LinkWithQuery } from '~/ui/utils/LinkWithQuery';

function TokenView({ data }: { data: DataType }) {
    const viewedData = {
        ...data,
        objType: data.objType,
        tx_digest: data.data.tx_digest,
        owner: getOwnerStr(data.owner),
        url: parseImageURL(data.data.contents),
    };

    const name = extractName(data?.data?.contents);

    const properties = Object.entries(viewedData.data?.contents).filter(
        ([key, value]) => key !== 'name' && checkIsPropertyType(value)
    );

    const structProperties = Object.entries(viewedData.data?.contents).filter(
        ([key, value]) => typeof value == 'object' && key !== 'id'
    );

    let structPropertiesDisplay: any[] = [];
    if (structProperties.length > 0) {
        structPropertiesDisplay = Object.values(structProperties).map(
            ([x, y]) => [x, JSON.stringify(y, null, 2)]
        );
    }

    const [fileType, setFileType] = useState<undefined | string>(undefined);

    useEffect(() => {
        const controller = new AbortController();
        genFileTypeMsg(viewedData.url, controller.signal)
            .then((result) => setFileType(result))
            .catch((err) => console.log(err));

        return () => {
            controller.abort();
        };
    }, [viewedData.url]);

    const [isImageFullScreen, setImageFullScreen] = useState<boolean>(false);

    const handlePreviewClick = useCallback(() => {
        setImageFullScreen(true);
    }, []);

    const genhref = (objType: string) => {
        const metadataarr = objType.split('::');
        const address = normalizeSuiAddress(metadataarr[0]);
        return `/objects/${address}?module=${metadataarr[1]}`;
    };

    return (
        <div>
            <div className={styles.descimagecontainer}>
                <div>
                    <h2 className={styles.header}>Description</h2>
                    <table className={styles.description}>
                        <tbody>
                            <tr>
                                <td>Type</td>
                                <td>
                                    <LinkWithQuery
                                        to={genhref(viewedData.objType)}
                                        className={styles.objecttypelink}
                                    >
                                        {trimStdLibPrefix(viewedData.objType)}
                                    </LinkWithQuery>
                                </td>
                            </tr>
                            <tr>
                                <td>Object ID</td>
                                <td id="objectID" className={styles.objectid}>
                                    <Longtext
                                        text={viewedData.id}
                                        category="objects"
                                        isLink={false}
                                    />
                                </td>
                            </tr>

                            {viewedData.tx_digest && (
                                <tr>
                                    <td>Last Transaction ID</td>
                                    <td>
                                        <Longtext
                                            text={viewedData.tx_digest}
                                            category="transactions"
                                            isLink
                                        />
                                    </td>
                                </tr>
                            )}

                            <tr>
                                <td>Version</td>
                                <td>{viewedData.version}</td>
                            </tr>

                            <tr>
                                <td>Owner</td>
                                <td id="owner">
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
                                </td>
                            </tr>
                            {viewedData.contract_id && (
                                <tr>
                                    <td>Contract ID</td>
                                    <td>
                                        <Longtext
                                            text={viewedData.contract_id.bytes}
                                            category="objects"
                                            isLink
                                        />
                                    </td>
                                </tr>
                            )}
                        </tbody>
                    </table>
                </div>
                {viewedData.url !== '' && (
                    <div className={styles.displaycontainer}>
                        <div className={styles.display}>
                            <DisplayBox
                                display={viewedData.url}
                                caption={
                                    name || trimStdLibPrefix(viewedData.objType)
                                }
                                fileInfo={fileType}
                                modalImage={[
                                    isImageFullScreen,
                                    setImageFullScreen,
                                ]}
                            />
                            <button
                                type="button"
                                onClick={handlePreviewClick}
                                className={styles.mobilepreviewmedia}
                            >
                                Preview Media <PreviewMediaIcon />
                            </button>
                        </div>
                        <div className={styles.metadata}>
                            {name && <h2 className={styles.header}>{name}</h2>}
                            {fileType && (
                                <p className={styles.header}>{fileType}</p>
                            )}
                            <button
                                type="button"
                                onClick={handlePreviewClick}
                                className={styles.desktoppreviewmedia}
                            >
                                Preview Media <PreviewMediaIcon />
                            </button>
                        </div>
                    </div>
                )}
            </div>
            {properties.length > 0 && (
                <>
                    <h2 className={styles.header}>Properties</h2>
                    <table className={styles.properties}>
                        <tbody>
                            {properties.map(([key, value], index) => (
                                <tr key={index}>
                                    <td>{key}</td>
                                    <td>{value}</td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </>
            )}
            {structProperties.length > 0 && (
                <ModulesWrapper
                    data={{
                        title: '',
                        content: structPropertiesDisplay,
                    }}
                />
            )}
            <h2 className={styles.header}>Dynamic Fields</h2>
            <div className={styles.ownedobjects}>
                <OwnedObjects id={viewedData.id} byAddress={false} />
            </div>
            <h2 className={styles.header}>Transactions </h2>
            <TxForID id={viewedData.id} category="object" />
        </div>
    );
}

export default TokenView;
