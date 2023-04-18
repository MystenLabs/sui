// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { ArrowUpRight16 } from '@mysten/icons';
import cl from 'classnames';
import { useMemo } from 'react';

import { useExplorerLink } from '../../hooks/useExplorerLink';
import { permissionsSelectors } from '../../redux/slices/permissions';
import { SuiApp, type DAppEntry } from './SuiApp';
import { SuiAppEmpty } from './SuiAppEmpty';
import { Button } from '_app/shared/ButtonUI';
import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useAppSelector } from '_hooks';
import { FEATURES } from '_src/shared/experimentation/features';
import { trackEvent } from '_src/shared/plausible';
import { prepareLinkToCompare } from '_src/shared/utils';

import st from './Playground.module.scss';

function AppsPlayGround() {
    const ecosystemApps =
        useFeature<DAppEntry[]>(FEATURES.WALLET_DAPPS).value ?? [];
    const allPermissions = useAppSelector(permissionsSelectors.selectAll);
    const linkToPermissionID = useMemo(() => {
        const map = new Map<string, string>();
        for (const aPermission of allPermissions) {
            map.set(prepareLinkToCompare(aPermission.origin), aPermission.id);
            if (aPermission.pagelink) {
                map.set(
                    prepareLinkToCompare(aPermission.pagelink),
                    aPermission.id
                );
            }
        }
        return map;
    }, [allPermissions]);
    const accountOnExplorerHref = useExplorerLink({
        type: ExplorerLinkType.address,
        useActiveAddress: true,
    });
    return (
        <div className={cl(st.container)}>
            <div className="flex justify-center">
                <Heading variant="heading6" color="gray-90" weight="semibold">
                    Playground
                </Heading>
            </div>
            <div className="my-4">
                <Button
                    variant="outline"
                    href={accountOnExplorerHref!}
                    text={
                        <div className="flex gap-1">
                            View your account on Sui Explorer <ArrowUpRight16 />
                        </div>
                    }
                    onClick={() => {
                        trackEvent('ViewExplorerAccount');
                    }}
                />
            </div>

            {ecosystemApps?.length ? (
                <div className="p-4 bg-gray-40 rounded-xl">
                    <Text variant="pBodySmall" color="gray-75" weight="normal">
                        Apps below are actively curated but do not indicate any
                        endorsement or relationship with Sui Wallet. Please
                        DYOR.
                    </Text>
                </div>
            ) : null}

            {ecosystemApps?.length ? (
                <div className={st.apps}>
                    {ecosystemApps.map((app) => (
                        <SuiApp
                            key={app.link}
                            {...app}
                            permissionID={linkToPermissionID.get(
                                prepareLinkToCompare(app.link)
                            )}
                            displayType="full"
                        />
                    ))}
                </div>
            ) : (
                <SuiAppEmpty displayType="full" />
            )}
        </div>
    );
}

export default AppsPlayGround;
export { default as ConnectedAppsCard } from './ConnectedAppsCard';
