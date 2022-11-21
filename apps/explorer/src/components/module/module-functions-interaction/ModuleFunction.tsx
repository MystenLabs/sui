// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiMoveNormalizedFunction, ObjectId } from '@mysten/sui.js';

import { DisclosureBox } from '~/ui/DisclosureBox';

export type ModuleFunctionProps = {
    packageId: ObjectId;
    moduleName: string;
    functionName: string;
    functionDetails: SuiMoveNormalizedFunction;
    defaultOpen?: boolean;
};
export function ModuleFunction({
    defaultOpen,
    functionName,
    functionDetails,
}: ModuleFunctionProps) {
    return (
        <DisclosureBox defaultOpen={defaultOpen} title={functionName}>
            <pre>{JSON.stringify(functionDetails, null, 2)}</pre>
        </DisclosureBox>
    );
}
