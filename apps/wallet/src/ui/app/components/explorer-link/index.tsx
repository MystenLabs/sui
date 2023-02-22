// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo, useCallback, useMemo } from 'react';

import {
    getObjectUrl,
    getAddressUrl,
    getTransactionUrl,
    getValidatorUrl,
} from './Explorer';
import { ExplorerLinkType } from './ExplorerLinkType';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { activeAccountSelector } from '_redux/slices/account';
import { trackEvent } from '_src/shared/plausible';

import type { ObjectId, SuiAddress, TransactionDigest } from '@mysten/sui.js';
import type { ReactNode } from 'react';

import st from './ExplorerLink.module.scss';

export type ExplorerLinkProps = (
    | {
          type: ExplorerLinkType.address;
          address: SuiAddress;
          useActiveAddress?: false;
      }
    | {
          type: ExplorerLinkType.address;
          useActiveAddress: true;
      }
    | { type: ExplorerLinkType.object; objectID: ObjectId }
    | { type: ExplorerLinkType.transaction; transactionID: TransactionDigest }
    | { type: ExplorerLinkType.validator; validator: SuiAddress }
) & {
    track?: boolean;
    children?: ReactNode;
    className?: string;
    title?: string;
    showIcon?: boolean;
};

function useAddress(props: ExplorerLinkProps) {
    const { type } = props;
    const isAddress = type === ExplorerLinkType.address;
    const isProvidedAddress = isAddress && !props.useActiveAddress;
    const activeAddress = useAppSelector(activeAccountSelector);
    return isProvidedAddress ? props.address : activeAddress;
}

function ExplorerLink(props: ExplorerLinkProps) {
    const { type, children, className, title, showIcon = true } = props;
    const address = useAddress(props);
    const [selectedApiEnv, customRPC] = useAppSelector(({ app }) => [
        app.apiEnv,
        app.customRPC,
    ]);

    const objectID = type === ExplorerLinkType.object ? props.objectID : null;
    const transactionID =
        type === ExplorerLinkType.transaction ? props.transactionID : null;
    const validator =
        type === ExplorerLinkType.validator ? props.validator : null;

    // fallback to localhost if customRPC is not set
    const customRPCUrl = customRPC || 'http://localhost:3000/';
    const explorerHref = useMemo(() => {
        switch (type) {
            case ExplorerLinkType.address:
                return (
                    address &&
                    getAddressUrl(address, selectedApiEnv, customRPCUrl)
                );
            case ExplorerLinkType.object:
                return (
                    objectID &&
                    getObjectUrl(objectID, selectedApiEnv, customRPCUrl)
                );
            case ExplorerLinkType.transaction:
                return (
                    transactionID &&
                    getTransactionUrl(
                        transactionID,
                        selectedApiEnv,
                        customRPCUrl
                    )
                );
            case ExplorerLinkType.validator:
                return (
                    validator &&
                    getValidatorUrl(validator, selectedApiEnv, customRPCUrl)
                );
        }
    }, [
        type,
        address,
        selectedApiEnv,
        customRPCUrl,
        objectID,
        transactionID,
        validator,
    ]);

    const handleclick = useCallback(() => {
        if (props.track) {
            trackEvent('ViewExplorerAccount');
        }
    }, [props.track]);

    if (!explorerHref) {
        return null;
    }

    return (
        <ExternalLink
            href={explorerHref}
            className={className}
            title={title}
            showIcon={false}
            onClick={handleclick}
        >
            <>
                {children}{' '}
                {showIcon && (
                    <Icon
                        icon={SuiIcons.ArrowLeft}
                        className={st.explorerIcon}
                    />
                )}
            </>
        </ExternalLink>
    );
}

export default memo(ExplorerLink);
