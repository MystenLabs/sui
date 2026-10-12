// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Pause and rotation of a master id. A registers with a rotation delay of one epoch; its cap is
// object(1,0) and its first forwarding address is
// 0x9c174786ca52 fafafafafafafafafa 00 0101...01. C is the rotation target.

//# init --addresses test=0x0 --accounts A B C

//# programmable --sender A --inputs mutshared(250) @A 1 --gas-budget 2000000000
//> 0: sui::forwarding_address::register(Input(0), Input(2));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs mutshared(250) object(1,0)
// The cap pauses the id.
//> 0: sui::forwarding_address::pause(Input(0), Input(1));

//# programmable --sender B --inputs 1000 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
// Deposits to a paused id fail.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs mutshared(250) 0x52ca8647179c
// Only the master or the cap can pause; a stranger cannot pause by master id.
//> 0: sui::forwarding_address::pause_by_master(Input(0), Input(1));

//# programmable --sender A --inputs mutshared(250) object(1,0)
// The cap unpauses and deposits flow again.
//> 0: sui::forwarding_address::unpause(Input(0), Input(1));

//# programmable --sender B --inputs 1000 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender A --inputs mutshared(250) object(1,0) @C
// Propose rotating the master to C. Deposits keep reaching A while the rotation is pending.
//> 0: sui::forwarding_address::propose_rotation(Input(0), Input(1), Input(2));

//# programmable --sender B --inputs 2000 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs mutshared(250) 0x52ca8647179c
// Not due yet: the delay is one epoch.
//> 0: sui::forwarding_address::finalize_rotation(Input(0), Input(1));

//# advance-epoch

//# programmable --sender B --inputs mutshared(250) 0x52ca8647179c
// Anyone can finalize once the delay has elapsed.
//> 0: sui::forwarding_address::finalize_rotation(Input(0), Input(1));

//# programmable --sender B --inputs 4000 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
// Deposits now reach C.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender A --inputs mutshared(250) object(1,0) @A
// A still holds the cap and can propose rotating back; C, now the master, cancels without it.
//> 0: sui::forwarding_address::propose_rotation(Input(0), Input(1), Input(2));

//# programmable --sender C --inputs mutshared(250) 0x52ca8647179c
//> 0: sui::forwarding_address::cancel_rotation_by_master(Input(0), Input(1));

//# programmable --sender A --inputs mutshared(250) object(1,0) @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
// A forwarding address cannot become a master.
//> 0: sui::forwarding_address::propose_rotation(Input(0), Input(1), Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-funds sui::balance::Balance<sui::sui::SUI> C
