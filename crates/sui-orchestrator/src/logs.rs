use std::{cmp::max, io::stdout};

use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};

#[derive(Default)]
pub struct LogsParser {
    pub node_errors: usize,
    pub node_panic: bool,
    pub client_errors: usize,
    pub client_panic: bool,
}

impl LogsParser {
    pub fn set_node_errors(&mut self, log: &str) {
        self.node_errors = log.matches(" ERROR").count();
        self.node_panic = log.contains("panic");
    }

    pub fn set_client_errors(&mut self, log: &str) {
        self.client_errors = max(self.client_errors, log.matches(" ERROR").count());
        self.client_panic = log.contains("panic");
    }

    pub fn aggregate(counters: Vec<Self>) -> Self {
        let mut highest = Self::default();
        for counter in counters {
            if counter.node_panic {
                return counter;
            } else if counter.client_panic {
                return counter;
            } else if counter.client_errors > highest.client_errors {
                highest = counter;
            } else if counter.node_errors > highest.node_errors {
                highest = counter;
            }
        }
        highest
    }

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
