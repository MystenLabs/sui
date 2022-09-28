// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject, isSuiMovePackage } from '@mysten/sui.js';
import cl from 'classnames';
import { memo, useMemo } from 'react';
import { Link } from 'react-router-dom';

import Field from './field';
import CopyToClipboard from '_components/copy-to-clipboard';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useMiddleEllipsis, useMediaUrl, useSuiObjectFields } from '_hooks';

import type { SuiObject as SuiObjectType } from '@mysten/sui.js';

import st from './SuiObject.module.scss';

export type SuiObjectProps = {
    obj: SuiObjectType;
    sendNFT?: boolean;
};

function SuiObject({ obj, sendNFT }: SuiObjectProps) {
    const { objectId } = obj.reference;
    const shortId = useMiddleEllipsis(objectId);
    const objType =
        (isSuiMoveObject(obj.data) && obj.data.type) || 'Move Package';
    const imgUrl = useMediaUrl(obj.data);
    const { keys } = useSuiObjectFields(obj.data);
    const suiMoveObjectFields = isSuiMoveObject(obj.data)
        ? obj.data.fields
        : null;

    const sendUrl = useMemo(
        () => `/send-nft?${new URLSearchParams({ objectId }).toString()}`,
        [objectId]
    );
    return (
        <div className={st.container}>
            <span className={st.id} title={objectId}>
                <CopyToClipboard txt={objectId}>{shortId}</CopyToClipboard>
            </span>
            <span className={st.type}>{objType}</span>
            <div className={st.content}>
                {imgUrl ? (
                    <>
                        <div className={st['img-container']}>
                            <img className={st.img} src={imgUrl} alt="NFT" />
                        </div>
                        <div className={st.splitter} />
                    </>
                ) : null}
                <div className={st.fields}>
                    {suiMoveObjectFields ? (
                        <>
                            {keys.map((aField) => (
                                <Field key={aField} name={aField}>
                                    {String(suiMoveObjectFields[aField])}
                                </Field>
                            ))}
                            {sendNFT ? (
                                <div>
                                    <Link
                                        className={cl('btn', st.send)}
                                        to={sendUrl}
                                    >
                                        Send NFT
                                    </Link>
                                </div>
                            ) : null}
                        </>
                    ) : null}

                    {isSuiMovePackage(obj.data) ? (
                        <Field name="disassembled">
                            {JSON.stringify(obj.data.disassembled).substring(
                                0,
                                50
                            )}
                        </Field>
                    ) : null}
                </div>
            </div>
            <ExplorerLink
                type={ExplorerLinkType.object}
                objectID={objectId}
                title="View on Sui Explorer"
                className={st['explorer-link']}
            />
        </div>
    );
}

export default memo(SuiObject);
