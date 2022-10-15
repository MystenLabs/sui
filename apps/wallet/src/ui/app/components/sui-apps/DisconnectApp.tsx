// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useState, useCallback } from 'react';

import { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import Overlay from '_components/overlay';
import { useAppDispatch } from '_hooks';
import { revokeAppPermissionByOrigin } from '_redux/slices/permissions';
import { trackEvent } from '_src/shared/plausible';

import type { SuiAddress } from '@mysten/sui.js';

import st from './DisconnectApp.module.scss';

type DisconnectAppProps = {
    name: string;
    icon?: string;
    link: string;
    linkLabel?: string;
    account?: string;
    id?: string;
    address?: SuiAddress;
    permissions: string[];
    disconnect?: boolean;
    pageLink?: string;
    setShowDisconnectApp: (showModal: boolean) => void;
};

function DisconnectApp({
    name,
    icon,
    link,
    address,
    linkLabel,
    account,
    id,
    permissions,
    pageLink,
    setShowDisconnectApp,
}: DisconnectAppProps) {
    const [showModal] = useState(true);
    const dispatch = useAppDispatch();

    // TODO: add loading state since this is async
    const revokeApp = useCallback(
        (e: React.MouseEvent<HTMLElement>) => {
            trackEvent('AppDisconnect', {
                props: { source: 'AppPage' },
            });
            dispatch(revokeAppPermissionByOrigin({ origin: link }));
            setShowDisconnectApp(false);
        },
        [dispatch, link, setShowDisconnectApp]
    );
    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowDisconnectApp}
            title="Connection Active"
        >
            <Content>
                <div className={cl(st.container)}>
                    <div className={st.details}>
                        <div className={st.icon}>
                            {icon ? (
                                <img src={icon} alt={name} />
                            ) : (
                                <div className={st.defaultImg}></div>
                            )}
                        </div>
                        <div className={st.info}>
                            <div className={st.name}>{name}</div>
                            <ExternalLink
                                href={pageLink || link}
                                title={name}
                                className={st.appLink}
                                showIcon={false}
                            >
                                {linkLabel || link}

                                <Icon
                                    icon={SuiIcons.ArrowRight}
                                    className={cl(
                                        st.arrowActionIcon,
                                        st.angledArrow
                                    )}
                                />
                            </ExternalLink>
                            {address && (
                                <ExplorerLink
                                    type={ExplorerLinkType.address}
                                    address={address}
                                    showIcon={false}
                                    className={st['explorer-link']}
                                >
                                    {address} <Icon icon={SuiIcons.Clipboard} />
                                </ExplorerLink>
                            )}
                        </div>

                        <div className={st.permissions}>
                            <div className={st.label}>
                                Permissions requested
                            </div>
                            <div className={st.permissionsList}>
                                {permissions.map((permission) => (
                                    <div className={st.access} key={permission}>
                                        <div className={st.accessIcon}>
                                            <Icon icon={SuiIcons.Checkmark} />
                                        </div>
                                        {permission}
                                    </div>
                                ))}
                            </div>
                        </div>
                    </div>
                </div>
                <div className={st.cta}>
                    <Button
                        className={cl('btn', st.ctaBtn, st.disconnectApp)}
                        onClick={revokeApp}
                    >
                        <div className={st.disconnect}>
                            <Icon icon={SuiIcons.Close} />
                        </div>
                        <span>Disconnect</span>
                    </Button>
                    <ExternalLink
                        href={pageLink || link}
                        title={name}
                        className={cl('btn', st.ctaBtn, st.view)}
                        showIcon={false}
                    >
                        View
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className={cl(st.arrowActionIcon, st.angledArrow)}
                        />
                    </ExternalLink>
                </div>
            </Content>
        </Overlay>
    );
}

export default DisconnectApp;
