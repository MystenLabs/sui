// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module enums::action {

    public enum Action has copy {
        Stop,
        MoveTo { x: u64, y: u64 },
        ChangeSpeed(u64),
    }

    public fun speed(action: &Action): u64 {
        match (action) {
            Action::MoveTo { x: speed, y: _ } => *speed,
            Action::ChangeSpeed(speed) => *speed,
            Action::Stop => abort 0,
        }
    }

    public fun increase_speed(action: &mut Action, new_speed: u64) {
        match (action) {
            Action::ChangeSpeed(speed) => *speed = new_speed,
            Action::Stop | Action::MoveTo { x:_, y:_ } => abort 0,
        }
    }

    public fun destroy_action(action: Action) {
        match (action) {
            Action::MoveTo { x: _, y: _ } => {},
            Action::ChangeSpeed(_) => {},
            Action::Stop => {},
        }
        // This is a no-op, but it ensures that the action is dropped.
    }
}