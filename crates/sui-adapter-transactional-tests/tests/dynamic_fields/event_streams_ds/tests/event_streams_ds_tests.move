// module event_streams_ds::event_streams_ds_tests;

// use sui::test_scenario::{Self, ctx};
// use event_streams_ds::hash_chains::{fetch, Obj, HashChainHead, update, init_for_testing};
// use std::debug::print;

// #[test]
// fun test_event_streams_ds() {
//     // Initialize a mock sender address
//     let addr1 = @0xA;

//     // Begins a multi transaction scenario with addr1 as the sender
//     let mut scenario = test_scenario::begin(addr1);

//     init_for_testing(scenario.ctx());

//     test_scenario::next_tx(&mut scenario, addr1);
//     {
//         // remove the Obj value from addr1's inventory
//         let mut obj = test_scenario::take_from_sender<Obj>(&scenario);
//         print<HashChainHead>(&fetch(&obj));
//         update(&mut obj);
//         print<HashChainHead>(&fetch(&obj));
//         update(&mut obj);
//         print<HashChainHead>(&fetch(&obj));
//         test_scenario::return_to_address<Obj>(addr1, obj);
//     };

//     // Cleans up the scenario object
//     test_scenario::end(scenario);
// }
