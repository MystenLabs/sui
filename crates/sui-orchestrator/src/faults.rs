// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use crate::client::Instance;

pub enum FaultsType {
    /// Permanently crash the maximum number of nodes from the beginning.
    Permanent,
    /// Crash and recover the first node. This option is mostly useful for debugging.
    CrashRecoveryOne,
    /// Progressively crash and recover nodes.
    CrashRecovery,
}

/// The actions to apply to the testbed, i.e., which instances to crash and recover.
#[derive(Default)]
pub struct Action {
    /// The instances to boot.
    pub boot: Vec<Instance>,
    /// The instances to kill.
    pub kill: Vec<Instance>,
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let killed = self.kill.len();
        if self.boot.is_empty() {
            write!(f, "{killed} nodes killed")
        } else {
            let booted = self.boot.len();
            write!(f, "{killed} nodes killed and {booted} nodes recovered")
        }
    }
}

pub struct FaultsSchedule {
    faults_type: FaultsType,
    alive: Vec<Instance>,
    dead: Vec<Instance>,
}

impl Display for FaultsSchedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let faults = self.alive.len() + self.dead.len();
        match self.faults_type {
            FaultsType::Permanent => write!(f, "{faults} permanently crashed"),
            FaultsType::CrashRecoveryOne => write!(f, "1 crash-recovery"),
            FaultsType::CrashRecovery => write!(f, "up to {faults} crash-recovery"),
        }
    }
}

impl FaultsSchedule {
    pub fn new(
        // The type of faulty behavior.
        faults_type: FaultsType,
        // The maximum number of instances that can b crashed.
        instances: Vec<Instance>,
    ) -> Self {
        Self {
            faults_type,
            alive: instances,
            dead: Vec::new(),
        }
    }
    pub fn update(&mut self) -> Action {
        match &self.faults_type {
            // Permanently crash the specified number of nodes.
            FaultsType::Permanent => Action {
                boot: Vec::new(),
                kill: self.alive.drain(..).collect(),
            },

            // Periodically crash and recover one node.
            FaultsType::CrashRecoveryOne => match self.dead.pop() {
                Some(instance) => {
                    self.alive.push(instance.clone());
                    Action {
                        boot: vec![instance],
                        kill: Vec::new(),
                    }
                }
                None => {
                    let instance = self.alive.pop().expect("The committee is empty");
                    self.dead.push(instance.clone());
                    Action {
                        boot: vec![instance],
                        kill: Vec::new(),
                    }
                }
            },

            // Periodically crash and recover nodes.
            FaultsType::CrashRecovery => {
                let max_faults = self.alive.len() + self.dead.len();
                let min_faults = max_faults / 3;

                // There are initially no dead nodes; kill a few of them. This branch is skipped
                // for committees smaller than 10 nodes.
                if self.dead.is_empty() && min_faults != 0 {
                    let kill: Vec<_> = self.alive.drain(0..min_faults).collect();
                    self.dead.extend(kill.clone());
                    Action {
                        boot: Vec::new(),
                        kill,
                    }

                // There are then a few dead nodes; kill a few of them again.  This branch is
                // skipped for committees smaller than 10 nodes.
                } else if self.dead.len() == min_faults && min_faults != 0 {
                    let kill: Vec<_> = self.alive.drain(0..min_faults).collect();
                    self.dead.extend(kill.clone());
                    Action {
                        boot: Vec::new(),
                        kill,
                    }

                // Kill the remaining nodes. The maximum number of nodes is killed for committees
                // smaller than 10 nodes.
                } else if !self.alive.is_empty() {
                    let kill: Vec<_> = self.alive.drain(..).collect();
                    self.dead.extend(kill.clone());
                    Action {
                        boot: Vec::new(),
                        kill,
                    }

                // Reboot all nodes.
                } else {
                    let boot: Vec<_> = self.dead.drain(..).collect();
                    self.alive.extend(boot.clone());
                    Action {
                        boot,
                        kill: Vec::new(),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod faults_tests {
    use crate::client::Instance;

    use super::{FaultsSchedule, FaultsType};

    #[test]
    fn crash_recovery_1_fault() {
        let faulty = vec![Instance::new_for_test("id".into())];
        let mut schedule = FaultsSchedule::new(FaultsType::CrashRecovery, faulty);

        let action = schedule.update();
        assert_eq!(action.boot.len(), 0);
        assert_eq!(action.kill.len(), 1);

        let action = schedule.update();
        assert_eq!(action.boot.len(), 1);
        assert_eq!(action.kill.len(), 0);

        let action = schedule.update();
        assert_eq!(action.boot.len(), 0);
        assert_eq!(action.kill.len(), 1);

        let action = schedule.update();
        assert_eq!(action.boot.len(), 1);
        assert_eq!(action.kill.len(), 0);
    }

    #[test]
    fn crash_recovery_2_faults() {
        let faulty = (0..2)
            .map(|i| Instance::new_for_test(i.to_string()))
            .collect();
        let mut schedule = FaultsSchedule::new(FaultsType::CrashRecovery, faulty);

        let action = schedule.update();
        assert_eq!(action.boot.len(), 0);
        assert_eq!(action.kill.len(), 2);

        let action = schedule.update();
        assert_eq!(action.boot.len(), 2);
        assert_eq!(action.kill.len(), 0);

        let action = schedule.update();
        assert_eq!(action.boot.len(), 0);
        assert_eq!(action.kill.len(), 2);

        let action = schedule.update();
        assert_eq!(action.boot.len(), 2);
        assert_eq!(action.kill.len(), 0);
    }

    #[test]
    fn crash_recovery() {
        for i in 3..33 {
            let max_faults = i;
            let min_faults = max_faults / 3;

            let instances = (0..max_faults)
                .map(|i| Instance::new_for_test(i.to_string()))
                .collect();
            let mut schedule = FaultsSchedule::new(FaultsType::CrashRecovery, instances);

            let action = schedule.update();
            assert_eq!(action.boot.len(), 0);
            assert_eq!(action.kill.len(), min_faults);

            let action = schedule.update();
            assert_eq!(action.boot.len(), 0);
            assert_eq!(action.kill.len(), min_faults);

            let action = schedule.update();
            assert_eq!(action.boot.len(), 0);
            assert_eq!(action.kill.len(), max_faults - 2 * min_faults);

            let action = schedule.update();
            assert_eq!(action.boot.len(), max_faults);
            assert_eq!(action.kill.len(), 0);

            let action = schedule.update();
            assert_eq!(action.boot.len(), 0);
            assert_eq!(action.kill.len(), min_faults);
        }
    }
}
