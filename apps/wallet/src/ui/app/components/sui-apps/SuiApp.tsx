// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useState, useCallback } from 'react';

import DisconnectApp from './DisconnectApp';
import ExternalLink from '_components/external-link';
import { useMiddleEllipsis } from '_hooks';
import { trackEvent } from '_src/shared/plausible';

import st from './SuiApp.module.scss';

type Displaytype = {
    displaytype: 'full' | 'card';
};

type SuiAppProps = {
    name?: string;
    description?: string;
    icon?: string;
    displaytype: 'full' | 'card';
    tags?: string[];
    link: string;
    account?: string;
    id?: string;
    pageLink?: string;
    permissions: string[];
    disconnect?: boolean;
};

const TRUNCATE_MAX_LENGTH = 18;

function SuiAppEmpty({ displaytype }: Displaytype) {
    return (
        <div className={cl(st.suiApp, st.suiAppEmpty, st[displaytype])}>
            <div className={st.icon}></div>
            <div className={st.info}>
                <div className={st.boxOne}></div>
                {displaytype === 'full' && (
                    <>
                        <div className={st.boxTwo}></div>
                        <div className={st.boxThree}></div>
                    </>
                )}
            </div>
        </div>
    );
}

function SuiApp({
    name,
    description,
    icon,
    displaytype,
    link,
    tags,
    id,
    account,
    pageLink,
    permissions,
    disconnect,
}: SuiAppProps) {
    const [showDisconnectApp, setShowDisconnectApp] = useState(false);
    const appData = {
        name: name || 'Unknown App',
        icon,
        link,
        id,
        permissions,
        pageLink,
    };

    const originLabel = useMiddleEllipsis(
        new URL(link).hostname,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_MAX_LENGTH - 1
    );

    const AppDetails = (
        <div className={cl(st.suiApp, st[displaytype])}>
            <div className={st.icon}>
                {icon ? (
                    <img src={icon} className={st.icon} alt={name} />
                ) : (
                    <div className={st.defaultImg}></div>
                )}
            </div>
            <div className={st.info}>
                <div className={st.title}>{name} </div>
                {displaytype === 'full' && (
                    <div className={st.description}>{description}</div>
                )}

                {displaytype === 'card' && (
                    <div className={st.link}>{originLabel}</div>
                )}

                {displaytype === 'full' && tags?.length && (
                    <div className={st.tags}>
                        {tags?.map((tag) => (
                            <div className={st.tag} key={tag}>
                                {tag}
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );

    const openApp = useCallback(
        (e: React.MouseEvent<HTMLElement>) => {
            setShowDisconnectApp(true);
        },
        [setShowDisconnectApp]
    );

    const onClickAppLink = useCallback(() => {
        trackEvent('AppOpen', {
            props: { name: name || link, source: 'AppPage' },
        });
    }, [name, link]);

    return (
        <>
            {showDisconnectApp && (
                <DisconnectApp
                    {...appData}
                    setShowDisconnectApp={setShowDisconnectApp}
                />
            )}
            {disconnect ? (
                <>
                    <div className={st.ecosystemApp} onClick={openApp}>
                        {AppDetails}
                    </div>
                </>
            ) : (
                <ExternalLink
                    href={pageLink || link}
                    title={name}
                    className={st.ecosystemApp}
                    showIcon={false}
                    onClick={onClickAppLink}
                >
                    {AppDetails}
                </ExternalLink>
            )}
        </>
    );
}

export default memo(SuiApp);
export { SuiAppEmpty };
