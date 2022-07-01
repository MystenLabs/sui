// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo, useMemo } from 'react';

import { Explorer } from './Explorer';
import { ExplorerLinkType } from './ExplorerLinkType';
import BsIcon from '_components/bs-icon';
import ExternalLink from '_components/external-link';
import { useAppSelector } from '_hooks';
import { activeAccountSelector } from '_redux/slices/account';

import type { ObjectId, SuiAddress, TransactionDigest } from '@mysten/sui.js';
import type { ReactNode } from 'react';

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
) & { children?: ReactNode; className?: string; title?: string };

function useAddress(props: ExplorerLinkProps) {
    const { type } = props;
    const isAddress = type === ExplorerLinkType.address;
    const isProvidedAddress = isAddress && !props.useActiveAddress;
    const activeAddress = useAppSelector(activeAccountSelector);
    return isProvidedAddress ? props.address : activeAddress;
}

function ExplorerLink(props: ExplorerLinkProps) {
    const { type, children, className, title } = props;
    const address = useAddress(props);
    const selectedApiEnv = useAppSelector(({ app }) => app.apiEnv);
    const objectID = type === ExplorerLinkType.object ? props.objectID : null;
    const transactionID =
        type === ExplorerLinkType.transaction ? props.transactionID : null;
    const explorerHref = useMemo(() => {
        switch (type) {
            case ExplorerLinkType.address:
                return (
                    address && Explorer.getAddressUrl(address, selectedApiEnv)
                );
            case ExplorerLinkType.object:
                return (
                    objectID && Explorer.getObjectUrl(objectID, selectedApiEnv)
                );
            case ExplorerLinkType.transaction:
                return (
                    transactionID &&
                    Explorer.getTransactionUrl(transactionID, selectedApiEnv)
                );
        }
    }, [type, address, objectID, transactionID, selectedApiEnv]);
    if (!explorerHref) {
        return null;
    }
    return (
        <ExternalLink
            href={explorerHref}
            className={className}
            title={title}
            showIcon={false}
        >
            <>
                {children} <BsIcon icon="box-arrow-up-right" />
            </>
        </ExternalLink>
    );
}

export default memo(ExplorerLink);
