// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use transaction_fuzzer::transaction_fuzzer::{
    add_stake, withdraw_stake, FuzzTestRunner, GenStateChange,
};

#[tokio::test]
async fn fuzz_dynamic_committee() {
    let num_operations = 10;

    // Add more actions here as we create them
    let actions: Vec<Box<dyn GenStateChange>> = vec![
        Box::new(add_stake::RequestAddStakeGen),
        Box::new(withdraw_stake::RequestWithdrawStakeGen),
    ];

    let mut runner = FuzzTestRunner::new().await;

    for i in 0..num_operations {
        if i % 5 == 0 {
            println!("Changing epoch");
            runner.change_epoch().await;
            continue;
        }
        let mut task = runner.select_next_operation(actions.as_slice());
        let effects = task.run(&mut runner).await.unwrap();
        task.pre_epoch_post_condition(&mut runner, &effects).await;
    }
}
