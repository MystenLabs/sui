// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::max, io::stdout};

use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};

/// A simple log analyzer counting the number of errors and panics.
#[derive(Default)]
pub struct LogsAnalyzer {
    /// The number of errors in the nodes' log files.
    pub node_errors: usize,
    /// Whether a node panicked.
    pub node_panic: bool,
    /// The number of errors int he clients' log files.
    pub client_errors: usize,
    /// Whether a client panicked.
    pub client_panic: bool,
}

impl LogsAnalyzer {
    /// Deduce the number of nodes errors from the logs.
    pub fn set_node_errors(&mut self, log: &str) {
        self.node_errors = log.matches(" ERROR").count();
        self.node_panic = log.contains("panic");
    }

    /// Deduce the number of clients errors from the logs.
    pub fn set_client_errors(&mut self, log: &str) {
        self.client_errors = max(self.client_errors, log.matches(" ERROR").count());
        self.client_panic = log.contains("panic");
    }

    /// Aggregate multiple log analyzers into one, based on the analyzer that found the
    /// most serious errors.
    pub fn aggregate(counters: Vec<Self>) -> Self {
        let mut highest = Self::default();
        for counter in counters {
            if counter.node_panic || counter.client_panic {
                return counter;
            } else if counter.client_errors > highest.client_errors
                || counter.node_errors > highest.node_errors
            {
                highest = counter;
            }
        }
        highest
    }

    /// Print a summary of the errors.
    pub fn print_summary(&self) {
        if self.node_panic {
            crossterm::execute!(
                stdout(),
                SetForegroundColor(Color::Red),
                SetAttribute(Attribute::Bold),
                Print("\nNode(s) panicked!\n"),
                ResetColor
            )
            .unwrap();
        } else if self.client_panic {
            crossterm::execute!(
                stdout(),
                SetForegroundColor(Color::Red),
                SetAttribute(Attribute::Bold),
                Print("\nClient(s) panicked!\n"),
                ResetColor
            )
            .unwrap();
        } else if self.node_errors != 0 || self.client_errors != 0 {
            crossterm::execute!(
                stdout(),
                SetAttribute(Attribute::Bold),
                Print(format!(
                    "\nLogs contain errors (node: {}, client: {})\n",
                    self.node_errors, self.client_errors
                )),
            )
            .unwrap();
        }
    }
}
