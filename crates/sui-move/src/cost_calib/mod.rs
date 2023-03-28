// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::runner::run_calib;
mod runner;

pub fn run_calibration(runs: usize, summarize: bool) {
    let res = run_calib(runs);

    if summarize {
        println!("-------------------------------------------------------------------");
        println!("{:30} {:20}", "Operation", "Avg Unprocessed time");
        println!("-------------------------------------------------------------------");
        res.iter()
            .for_each(|(oper, (_, summary))| println!("{:30} {:5} ", oper, summary));
    } else {
        for (oper, (values, summary)) in res {
            println!("-------------------------------------------------------------------");
            println!("{:10} {:10}", "Operation:", oper);
            println!("-------------------------------------------------------------------");

            println!("Subject    Baseline   Diff ({:5} avg)", summary);
            for (subject, baseline) in values {
                println!(
                    "{:5}      {:5}  {:5}",
                    subject,
                    baseline,
                    subject - baseline
                );
            }
        }
    }
}
