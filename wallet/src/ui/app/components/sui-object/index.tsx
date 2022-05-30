// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject, isSuiMovePackage } from '@mysten/sui.js';
import { memo, useMemo } from 'react';

import Field from './field';
import BsIcon from '_components/bs-icon';
import CopyToClipboard from '_components/copy-to-clipboard';
import { useMiddleEllipsis, useMediaUrl, useSuiObjectFields } from '_hooks';
import { Explorer } from '_redux/slices/sui-objects/Explorer';

import type { SuiObject as SuiObjectType } from '@mysten/sui.js';

import st from './SuiObject.module.scss';

export type SuiObjectProps = {
    obj: SuiObjectType;
};

function SuiObject({ obj }: SuiObjectProps) {
    const { objectId } = obj.reference;
    const shortId = useMiddleEllipsis(objectId);
    const objType =
        (isSuiMoveObject(obj.data) && obj.data.type) || 'Move Package';
    const imgUrl = useMediaUrl(obj.data);
    const { keys } = useSuiObjectFields(obj.data);
    const suiMoveObjectFields = isSuiMoveObject(obj.data)
        ? obj.data.fields
        : null;
    const explorerUrl = useMemo(
        () => Explorer.getObjectUrl(objectId),
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
                    {suiMoveObjectFields
                        ? keys.map((aField) => (
                              <Field key={aField} name={aField}>
                                  {String(suiMoveObjectFields[aField])}
                              </Field>
                          ))
                        : null}
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
            <a
                href={explorerUrl}
                target="_blank"
                className={st['explorer-link']}
                rel="noreferrer"
            >
                <BsIcon
                    title="View on Sui Explorer"
                    icon="box-arrow-up-right"
                />
            </a>
        </div>
    );
}

export default memo(SuiObject);
