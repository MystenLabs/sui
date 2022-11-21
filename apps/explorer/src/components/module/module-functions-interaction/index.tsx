// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { ModuleFunction } from './ModuleFunction';

import type { ObjectId } from '@mysten/sui.js';

import { useNormalizedMoveModule } from '~/hooks/useNormalizedMoveModule';
import { Banner } from '~/ui/Banner';

export type ModuleFunctionsInteractionProps = {
    packageId: ObjectId;
    moduleName: string;
};

export function ModuleFunctionsInteraction({
    packageId,
    moduleName,
}: ModuleFunctionsInteractionProps) {
    const {
        data: normalizedModule,
        error,
        isLoading,
    } = useNormalizedMoveModule(packageId, moduleName);
    const executableFunctions = useMemo(() => {
        if (!normalizedModule) {
            return [];
        }
        return Object.entries(normalizedModule.exposed_functions)
            .filter(([_, anFn]) => anFn.is_entry)
            .map(([fnName, details]) => ({ name: fnName, details }));
    }, [normalizedModule]);

    if (error) {
        return (
            <Banner variant="error" fullWidth>
                Error loading module <strong>{moduleName}</strong> details.
            </Banner>
        );
    }

    return !isLoading && executableFunctions.length ? (
        <div className="flex flex-col gap-3">
            {executableFunctions.map(({ name, details }) => (
                <ModuleFunction
                    key={name}
                    functionName={name}
                    functionDetails={details}
                    moduleName={moduleName}
                    packageId={packageId}
                />
            ))}
        </div>
    ) : null;
}
