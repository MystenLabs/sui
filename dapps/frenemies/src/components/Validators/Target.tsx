// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Goal } from "../../network/types";

interface Props {
    goal: Goal
}

export function Target({ goal }: Props) {
    return (
        <div className="text-sm text-left">{note(goal)}</div>
    );
}

/**
 * Note marking the assignment details (eg where you need to get this validator).
 * Not used anywhere really, so keeping inside the component.
 * @param goal
 */
function note(goal: Goal): string {
    switch (goal) {
        case Goal.Friend: return 'Friend, Goal 1-13 Rank';
        case Goal.Neutral: return 'Neutral, Goal 14-25 Rank';
        case Goal.Enemy: return 'Enemy, Goal 26-40 Rank';
    }
}
