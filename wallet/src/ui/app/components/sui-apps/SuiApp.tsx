// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';

import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';

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
    permissions?: string[];
    disconnect?: boolean;
};

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
    account,
    permissions,
}: SuiAppProps) {
    return (
        <ExternalLink
            href={link}
            title={name}
            className={st.ecosystemApp}
            showIcon={false}
        >
            <div className={cl(st.suiApp, st[displaytype])}>
                <div className={st.icon}>
                    {icon ? (
                        <img src={icon} className={st.icon} alt={name} />
                    ) : (
                        <div className={st.defaultImg}></div>
                    )}
                    {displaytype === 'card' && (
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className={cl(
                                st.arrowActionIcon,
                                st.angledArrow,
                                st.externalLinkIcon
                            )}
                        />
                    )}
                </div>
                <div className={st.info}>
                    <div className={st.title}>
                        {name}{' '}
                        {displaytype === 'full' && (
                            <Icon
                                icon={SuiIcons.ArrowRight}
                                className={cl(
                                    st.arrowActionIcon,
                                    st.angledArrow
                                )}
                            />
                        )}
                    </div>
                    {displaytype === 'full' && (
                        <div className={st.description}>{description}</div>
                    )}

                    {displaytype === 'card' && (
                        <div className={st.link}>{link}</div>
                    )}

                    {displaytype === 'full' && tags?.length && (
                        <div className={st.tags}>
                            {tags?.map((tag, index) => (
                                <div className={st.tag} key={index}>
                                    {tag}
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </div>
        </ExternalLink>
    );
}

export default memo(SuiApp);
export { SuiAppEmpty };
