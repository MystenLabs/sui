---
title: End-to-End Tutorial to Set Up and Play TicTacToe on Sui
---

This tutorial demonstrates the end-to-end process for starting a
Sui network locally, connecting to it through the [Sui CLI client](../build/cli-client.md), publishing a TicTacToe game written in [Move](../build/move/index.md) on Sui,
and playing it to the end.

## Set up

1. [Install Sui binaries](../build/install.md#install-sui-binaries) and
   [download Sui source code](../build/install.md#source-code).
1. [Create Sui genesis](../build/cli-client.md#genesis) by running the
   `sui genesis` command.
1. [Start the Sui network](../build/cli-client.md#starting-the-network) by
   running the `sui start` command.

After completing these steps, you have a running local Sui instance and
the `sui client` command used in the remainder of this tutorial in your path.
Simply leave the terminal with Sui running and start a new terminal for the
remainder of this tutorial.

This tutorial models gas fees under a simplified schema. In practice, the Sui
network charges for gas using its native currency SUI. This transaction fee
equals the computational effort of executing operations on the Sui network (i.e.
gas units) times the price of gas in the SUI currency (i.e. the gas price).

When you complete the setup steps, you can either use the following script to publish and run the sample code, or perform each step manually. Using the script is optional. To manually run each step, follow the steps starting in the [Gather addresses and gas objects](#gather-addresses-and-gas-objects) section.

## Quick script
If you prefer not to enter command step by step, or need to go through it multiple
times (such as when you change some Move source code), the following automated script
may be useful to save some time.
Run this script from the project repo root.
```sh
#!/bin/bash
# a bash script to automate the process of publishing the game package
# this script should be run at root of the repo

# assign address
CLIENT_ADDRESS=$(sui client addresses | tail -n +2)
ADMIN=`echo "${CLIENT_ADDRESS}" | head -n 1`
PLAYER_X=`echo "${CLIENT_ADDRESS}" | sed -n 2p`
PLAYER_O=`echo "${CLIENT_ADDRESS}" | sed -n 3p`
# gas id
IFS='|'
ADMIN_GAS_INFO=$(sui client gas --address $ADMIN | sed -n 3p)
read -a tmparr <<< "$ADMIN_GAS_INFO"
ADMIN_GAS_ID=`echo ${tmparr[0]} | xargs`

X_GAS_INFO=$(sui client gas --address $PLAYER_X | sed -n 3p)
read -a tmparr <<< "$X_GAS_INFO"
X_GAS_ID=`echo ${tmparr[0]} | xargs`

O_GAS_INFO=$(sui client gas --address $PLAYER_O | sed -n 3p)
read -a tmparr <<< "$O_GAS_INFO"
O_GAS_ID=`echo ${tmparr[0]} | xargs`

# publish games
certificate=$(sui client publish ./sui_programmability/examples/games --gas $ADMIN_GAS_ID --gas-budget 30000)

package_id_identifier="The newly published package object ID:"
res=$(echo $certificate | awk -v s="$package_id_identifier" 'index($0, s) == 1')
IFS=':'
read -a resarr <<< "$res"
echo ${resarr[1]}
PACKAGE_ID=`echo ${resarr[1]} | xargs`

echo "package id: $PACKAGE_ID"
# Playing TicTacToe

# create a game
sui client call --package $PACKAGE_ID --module tic_tac_toe --function create_game --args $PLAYER_X $PLAYER_O --gas $ADMIN_GAS_ID --gas-budget 1000

# start playing the game ...
```

## Gather addresses and gas objects

In the new terminal session you started, use the following command in the Sui CLI client to view the addresses available:

```shell
$ sui client addresses
```

The response lists the addresses available. Choose three to use for the tutorial. Call them ADMIN, PLAYER_X, and PLAYER_O.

To make it easier to use these address values and gas object, create an environment variable for each address so you don't have to manually add them each time:

```bash
export ADMIN=0x008e9c621f4fdb210b873aab59a1e5bf32ddb1d33ee85eb069b348c234465106
export PLAYER_X=0x011a285261b9f8d10a0c7ecb4c0dbe6d396825768dba38c3056809472736e521
export PLAYER_O=0x4ab708d1a4160fa0fdbf359691764e16380444ddb48d2b8856a169594a9baa55
```

Next, determine the gas objects associated with each address:
```bash
sui client gas $ADMIN
```

```bash
sui client gas $PLAYER_X
```

```bash
sui client gas $PLAYER_O
```

The tutorial uses only one gas object per address. Choose the object to use, and then create variables for them as well.

```bash
export ADMIN_GAS=0x1aa482ad8c6240cda3097a4aa13ad5bfb27bf6052133c01f79c8b4ea0aaa0601
export X_GAS=0x3fd0e889ee56152cdbd5fa5b5dab78ddc66d127930f5173ae7b5a9ac3e17dd6d
export O_GAS=0x51ec7820e82035a5de7b4f3ba2a3813ea099dca1867876f4177a1fa1d1efe022
```

## Publish the TicTacToe game on Sui
To keep this tutorial simple, use the TicTacToe game example from [tic_tac_toe.move](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/games/sources/tic_tac_toe.move).

To publish the game, run the publish command and specify the path to the location of the game package:
```shell
$ sui client publish ./sui/sui_programmability/examples/games --gas $ADMIN_GAS --gas-budget 30000
```

The response resembles the following:

```shell
----- Certificate ----
Signed Authorities : ...
Transaction Kind : Publish
Gas Budget : 30000
----- Publish Results ----
The newly published package object: (A613A7FF8CB03E0DFC0D157E232BBA50C5F19D17, SequenceNumber(1), o#fb70b9d1ac25250a00e35031289f89cd9c9d739f8663e1cc8ed739095a104e68)
List of objects created by running module initializers: []
Updated Gas : Coin { id: 38B89FE9F4A4823F1406938E87A8767CBD7F0B93, value: 92939 }
```
The package successfully published. Some gas was charged: the initial gas value was 100000, now it's 92939. The newly published package has the ID `A613A7FF8CB03E0DFC0D157E232BBA50C5F19D17`. Note that this ID is different than ID for the package you publish.

```bash
export PACKAGE=A613A7FF8CB03E0DFC0D157E232BBA50C5F19D17
```

## Playing TicTacToe
As mentioned earlier, the game requires three participants: Admin, PlayerX and PlayerO. At a high level, the game works as follows:
 1. The admin creates a game and specifies the addresses of the two players. This also creates two capability objects and grants each of the addresses permission to participate in the same game.
 1. Each player takes turns to send a *Mark* object to the admin that indicates their move.
 1. The admin receives the marks (in practice, this is done through monitoring events), and positions the mark on the game board.
 1. (2) and (3) repeat until the game ends.

Because the admin owns the game board, each individual player cannot place a mark directly on it. The players don't own the object so can't mutate it. Each mark placement consists of two steps. Each player first sends a mark, and then the admin places the mark. View an example game play in the [tic_tac_toe_tests](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/games/tests/tic_tac_toe_tests.move) file.

Now let's begin the game!

First, create a game with the following command:

```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function create_game --args $PLAYER_X $PLAYER_O --gas $ADMIN_GAS --gas-budget 1000
```

The response resembles the following:

```shell
----- Certificate ----
Signed Authorities : ...
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x1aa482ad8c6240cda3097a4aa13ad5bfb27bf6052133c01f79c8b4ea0aaa0601
Module : TicTacToe
Function : create_game
Object Arguments : []
Pure Arguments : [[123, 97, 218, 106, 172, 237, 127, 40, 193, 24, 125, 153, 137, 85, 241, 4, 100, 190, 174, 85], [37, 28, 242, 36, 182, 186, 58, 1, 157, 4, 182, 4, 19, 87, 194, 4, 144, 247, 163, 34]]
Type Arguments : []
----- Transaction Effects ----
Status : Success { gas_used: 284 }
Created Objects:
5851B7EA07B93E68696BC0CF811D2E266DFB880D SequenceNumber(1) o#02b1de02a055a8edb62c7677652f3ff1d92b40202504bd2da50e8d900b266a8e
A6D3B507D4533822E690291166891D42694A2721 SequenceNumber(1) o#930533ef8b324909c65d3586c73a6db1b7ee116704d6bc986a2c3a8f51d8bf10
F1B8161BD97D3CD6627E739AD675089C5ACFB452 SequenceNumber(1) o#1c92bdf7646cad2a65357fadf60605abc1669dfaa2de0a08709b706ac4b69c8f
Mutated Objects:
38B89FE9F4A4823F1406938E87A8767CBD7F0B93 SequenceNumber(2) o#26dbaf7ec2032a6270a45498ad46ac0b1ddbc361fcff20cadafaf5d39b8181b1
```

The preceding call created three objects. For each object, it printed out a tuple of three values (object_id, version, object_digest). Object ID is what we care about here. Since we don't have a real application here to display things for us, we need a bit of object printing magic to figure out which object is which. Let's print out the metadata of each created object (replace the object ID with what you see on your screen):

```bash
sui client object 5851B7EA07B93E68696BC0CF811D2E266DFB880D
Owner: AddressOwner(k#0x4ab708d1a4160fa0fdbf359691764e16380444ddb48d2b8856a169594a9baa55)
Version: 1
ID: 5851B7EA07B93E68696BC0CF811D2E266DFB880D
Readonly: false
Type: 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17::TicTacToe::MarkMintCap

sui client object A6D3B507D4533822E690291166891D42694A2721
Owner: AddressOwner(k#0x011a285261b9f8d10a0c7ecb4c0dbe6d396825768dba38c3056809472736e521)
Version: 1
ID: A6D3B507D4533822E690291166891D42694A2721
Readonly: false
Type: 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17::TicTacToe::MarkMintCap

sui client object F1B8161BD97D3CD6627E739AD675089C5ACFB452
Owner: AddressOwner(k#0x008e9c621f4fdb210b873aab59a1e5bf32ddb1d33ee85eb069b348c234465106)
Version: 1
ID: F1B8161BD97D3CD6627E739AD675089C5ACFB452
Readonly: false
Type: 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17::TicTacToe::TicTacToe
```

There are two `MarkMintCap` objects (for capability of minting a mark for each player) and a TicTacToe object (the game object). Take a look at each of the `Owner` fields, and you will see that:
 1. `MarkMintCap` Object `5851B7EA07B93E68696BC0CF811D2E266DFB880D` is owned by PLAYER_O.
 1. `MarkMintCap` Object `A6D3B507D4533822E690291166891D42694A2721` is owned by PLAYER_X.
 1. `TicTacToe` Object `F1B8161BD97D3CD6627E739AD675089C5ACFB452` is owned by ADMIN.

We add the above three object IDs to these environment variables:

```
$ export XCAP=A6D3B507D4533822E690291166891D42694A2721
export OCAP=5851B7EA07B93E68696BC0CF811D2E266DFB880D
export GAME=F1B8161BD97D3CD6627E739AD675089C5ACFB452
```

By convention, PlayerX goes first. PlayerX wants to put a mark at the center of the gameboard ((1, 1)). This needs to take two steps. First PlayerX creates a Mark object with the placement intention and send it to the admin.
We will call the `send_mark_to_game` function in `TicTacToe`, whose signature looks like this:

```
public entry fun send_mark_to_game(cap: &mut MarkMintCap, game_address: address, row: u64, col: u64, ctx: &mut TxContext);
```

The `cap` argument will be PlayerX's capability object (XCAP), and `game_address` argument will be the admin's address (ADMIN):

```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $XCAP $ADMIN 1 1 --gas $X_GAS --gas-budget 1000
```

And its output:

```shell
----- Certificate ----
Signed Authorities : ...
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0x1aa482ad8c6240cda3097a4aa13ad5bfb27bf6052133c01f79c8b4ea0aaa0601
Module : TicTacToe
Function : send_mark_to_game
Object Arguments : [(A6D3B507D4533822E690291166891D42694A2721, SequenceNumber(1), o#930533ef8b324909c65d3586c73a6db1b7ee116704d6bc986a2c3a8f51d8bf10)]
Pure Arguments : [[236, 245, 60, 226, 45, 27, 47, 181, 136, 87, 57, 36, 5, 126, 154, 221, 173, 29, 131, 133], [1, 0, 0, 0, 0, 0, 0, 0], [1, 0, 0, 0, 0, 0, 0, 0]]
Type Arguments : []
----- Transaction Effects ----
Status : Success { gas_used: 102 }
Created Objects:
AE3CE9176F1A8C1F21D922722486DF667FA00394 SequenceNumber(1) o#d40c0e3c74a2badd60c754456f0b830348bf7df629b0762e8b841c7cab5f4b2e
Mutated Objects:
...
```

The above call created a Mark object, with ID `AE3CE9176F1A8C1F21D922722486DF667FA00394`, and it was sent to the admin.
The admin can now place the mark on the gameboard. The function to place the mark looks like this:

```rust
public entry fun place_mark(game: &mut TicTacToe, mark: Mark, ctx: &mut TxContext);
```

The first argument is the game board, and the second argument is the mark the admin just received from the player. We will call this function (replace the second argument with the Mark object ID above):

```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0xAE3CE9176F1A8C1F21D922722486DF667FA00394 --gas $ADMIN_GAS --gas-budget 1000
```

The gameboard now looks like this (this won't be printed out, so keep it in your imagination):

```
_|_|_
_|X|_
 | |
```

PlayerO now tries to put a mark at (0, 0):

```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $OCAP $ADMIN 0 0 --gas $O_GAS --gas-budget 1000
```

With output like:

```shell
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 102 }
Created Objects:
7A16D266DAD41145F34649258BC1F744D147BF2F SequenceNumber(1) o#58cb018be98dd828c10f5b2045329f6ec4dab56c5a90e719ad225f0bc195908a
...
```

Note, in this second call, the second argument comes from the created objects in the first call.
```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0x7A16D266DAD41145F34649258BC1F744D147BF2F --gas $ADMIN_GAS --gas-budget 1000
```

With output like:

```shell
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 679 }
...
```

The gameboard now looks like this:

```
O|_|_
_|X|_
 | |
```

PlayerX puts a mark at (0, 2):

```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $XCAP $ADMIN 0 2 --gas $X_GAS --gas-budget 1000
```

With output like:

```shell
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 102 }
Created Objects:
2875D50BD9021ED2009A1278C7CB6D4C876FFF6A SequenceNumber(1) o#d4371b72e77bfc07bd088a9113ef7bf870198066649f6c9e9e4abf5f7a7fbd2a
...
```

Then run:

```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0x2875D50BD9021ED2009A1278C7CB6D4C876FFF6A --gas $ADMIN_GAS --gas-budget 1000
```

The gameboard now looks like this:

```
O|_|X
_|X|_
 | |
```

PlayerO places a mark at (1, 0):

```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $OCAP $ADMIN 1 0 --gas $O_GAS --gas-budget 1000
```

With output like:

```shell
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 102 }
Created Objects:
4F7391F172063D87013DD9DC95B8BD45C35FD2D9 SequenceNumber(1) o#ee7ba8ea66e574d7636e159da9f33c96d724d71f141a57a1e4929b2b928c298d
...
```

Now run:
```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0x4F7391F172063D87013DD9DC95B8BD45C35FD2D9 --gas $ADMIN_GAS --gas-budget 1000
...
```
The gameboard now looks like:
```
O|_|X
O|X|_
 | |
```

This is a chance for PlayerX to win! X now mints the winning mark at (2, 0):

```shell
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $XCAP $ADMIN 2 0 --gas $X_GAS --gas-budget 1000
```

And its output:

```shell
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 102 }
Created Objects:
AA7A6624E16E5E447801462FF6614013FC4AD156 SequenceNumber(1) o#e5e1b15f03531db118efaa9667244b876f32e7ad2cc17bdbc7d4cb1eaca1560d
...
```

And then finally the admin places the winning mark:
```shell
   $ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0xAA7A6624E16E5E447801462FF6614013FC4AD156 --gas $ADMIN_GAS --gas-budget 1000
```

With output:
```shell
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 870 }
Created Objects:
54B58C0D5B14A269B1CD424B3CCAB1E315C43343 SequenceNumber(1) o#7a093db738f6708c33d264d023c0eb07bcd9d22f038dbdcf1cbfdad50b0c1e42
Mutated Objects:
...
```

Cool! The last transaction created a new object. Let's find out what object was created:
```shell
$ sui client object 54B58C0D5B14A269B1CD424B3CCAB1E315C43343
```

See output resembling:
```shell
Owner: AddressOwner(k#0x011a285261b9f8d10a0c7ecb4c0dbe6d396825768dba38c3056809472736e521)
Version: 1
ID: 54B58C0D5B14A269B1CD424B3CCAB1E315C43343
Readonly: false
Type: 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17::TicTacToe::Trophy
```

PlayerX has received a Trophy object and hence won the game!

This concludes the tutorial.
