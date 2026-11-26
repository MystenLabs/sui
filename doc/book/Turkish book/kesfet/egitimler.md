---
description: Sui'de TicTacToe Oyununu Kurmak ve Oynamak İçin Baştan Sona Eğitim
---

# Eğitimler

Bu eğitimde, bir Sui ağını yerel olarak başlatma, CLI istemci uygulamamız aracılığıyla ağa bağlanma, Sui'de Move ile yazılmış bir TicTacToe oyununu yayınlama ve sonuna kadar oynama sürecini uçtan uca gösteriyoruz.

### Kurulum <a href="#set-up" id="set-up"></a>

1. [Sui binary'i kurun](https://docs.sui.io/devnet/build/install#binaries) ve [Sui kaynak kodunu indirin](https://docs.sui.io/devnet/build/install#source-code).
2. `sui genesis` komutunu çalıştırarak [Sui genesis'i oluşturun](https://docs.sui.io/devnet/build/cli-client#genesis).
3. `sui start` komutunu çalıştırarak [Sui ağını başlatın](https://docs.sui.io/devnet/build/cli-client#starting-the-network).

Bu adımları tamamladıktan sonra, çalışan bir yerel Sui örneğine ve bu eğitimin geri kalanında kullanılan sui istemci komutuna sahip olacaksınız. Sui'nin çalıştığı terminali bırakın ve bu eğitimin geri kalanı için yeni bir terminal başlatın.

Bu eğitimde gas ücretleri basitleştirilmiş bir şema altında modellenmektedir. Uygulamada, Sui ağı gas için kendi para birimi SUI'yi kullanarak ücret almaktadır. Bu işlem ücreti, Sui ağındaki işlemleri yürütmek için harcanan hesaplama çabası (yani gaz birimleri) ile SUI para birimindeki gas fiyatının (yani gas fiyatı) çarpımına eşittir.

Kurulum adımlarını tamamladığınızda, örnek kodu yayınlamak ve çalıştırmak için aşağıdaki komut dosyasını kullanabilir veya her adımı manuel olarak gerçekleştirebilirsiniz. Komut dosyasını kullanmak isteğe bağlıdır. Her bir adımı manuel olarak çalıştırmak için [Adresleri ve gas nesnelerini topla](https://docs.sui.io/devnet/explore/tutorials#gather-addresses-and-gas-objects) bölümünde başlayan adımları izleyin.

### Hızlı script <a href="#quick-script" id="quick-script"></a>

Komutu adım adım girmeyi tercih etmiyorsanız veya birden çok kez gözden geçirmeniz gerekiyorsa (örneğin, bazı Move kaynak kodlarını değiştirdiğinizde), aşağıdaki otomatik komut dosyası biraz zaman kazanmak için yararlı olabilir. Bu betiği proje deposunun kök dizininden çalıştırın.

```
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
certificate=$(sui client publish --path ./sui_programmability/examples/games --gas $ADMIN_GAS_ID --gas-budget 30000)

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

### Adresleri ve gas nesnelerini toplayın <a href="#gather-addresses-and-gas-objects" id="gather-addresses-and-gas-objects"></a>

In the new terminal session you started, use the following command in the Sui CLI client to view the addresses we own:

Which results in output resembling:

```
Showing 5 results.
ECF53CE22D1B2FB588573924057E9ADDAD1D8385
7B61DA6AACED7F28C1187D998955F10464BEAE55
251CF224B6BA3A019D04B6041357C20490F7A322
DB4C7667636471AFF396B900EB7B63FACAF629B6
A6BBB1930E01495EE93CE912EA01C29695E07890
```

Note that since these addresses are randomly generated, they are different from the values you see when you run the command. We need three addresses to play TicTacToe. Let's pick the first three addresses. Let's call them ADMIN, PLAYER\_X and PLAYER\_O.

Since we will be using these addresses and gas objects repeatedly in the rest of this tutorial, let's make them environment variables so that we don't have to retype them every time:

```
$ export ADMIN=ECF53CE22D1B2FB588573924057E9ADDAD1D8385
export PLAYER_X=7B61DA6AACED7F28C1187D998955F10464BEAE55
export PLAYER_O=251CF224B6BA3A019D04B6041357C20490F7A322
```

For each of these addresses, let's discover the gas objects associated with each address:

```
$ sui client gas --address $ADMIN
                Object ID                 |  Version   |  Gas Value
----------------------------------------------------------------------
 38B89FE9F4A4823F1406938E87A8767CBD7F0B93 |     0      |   100000
 4790500A28AB5B4F9A3988E2A5E201D56996CBB0 |     0      |   100000
 6AB7D15F41B28FF1EBF6D32499214BBD9035D1EB |     0      |   100000
 800F2704E22637A036C4325B539D711BB83CA6C2 |     0      |   100000
 D2F52301D5343DD2C1FA076401BC6283C3E4AA34 |     0      |   100000
$ sui client gas --address $PLAYER_X
                Object ID                 |  Version   |  Gas Value
----------------------------------------------------------------------
 6F675038CAA48184707DBBE95ACFBA2030E87CD8 |     0      |   100000
 80C91F0B31EFBC1C7BF639A531301AAF3A1D3AB6 |     0      |   100000
 9FED1FC3D21F284DC53DE87C0E19718971D96D8C |     0      |   100000
 E293F935F015C23216867442DB4E712518E7CAB7 |     0      |   100000
 F19384C06AE538F9C3C9D9762002B4DAEA49FE3A |     0      |   100000
$ sui client gas --address $PLAYER_O
                Object ID                 |  Version   |  Gas Value
----------------------------------------------------------------------
 2110ADFB7BAF889A05EA6F5889AF7724299F9BED |     0      |   100000
 8C04A5D8D62155B9E90093D6CB300DA304B9E581 |     0      |   100000
 9602B7C0869E7E5AB314FB3D99395A8C640E0E34 |     0      |   100000
 A0DBF58C3801EC2FEDA1D039E190A6B31A25B199 |     0      |   100000
 D5EBB8A19A35874A18B7A1D883EBFC8D897F5693 |     0      |   100000
```

We only need one gas object per address. So let's pick the first gas object of each address. In the above example, it's `38B89FE9F4A4823F1406938E87A8767CBD7F0B93`, `6F675038CAA48184707DBBE95ACFBA2030E87CD8` and `2110ADFB7BAF889A05EA6F5889AF7724299F9BED`, respectively. Again, you will see different IDs. Let's also add them to our environment variables:

```
$ export ADMIN_GAS=38B89FE9F4A4823F1406938E87A8767CBD7F0B93
export X_GAS=6F675038CAA48184707DBBE95ACFBA2030E87CD8
export O_GAS=2110ADFB7BAF889A05EA6F5889AF7724299F9BED
```

### Publish the TicTacToe game on Sui <a href="#publish-the-tictactoe-game-on-sui" id="publish-the-tictactoe-game-on-sui"></a>

To keep this tutorial simple, use the TicTacToe game we implemented in [tic\_tac\_toe.move](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/games/sources/tic\_tac\_toe.move).

Find even more examples in the Sui repository. Of course, you are welcome to write your own package.

To publish the game, we run the publish command and specify the path to the source code of the game package:

```
$ sui client publish --path ./sui/sui_programmability/examples/games --gas $ADMIN_GAS --gas-budget 30000
```

Which yields results resembling:

```
----- Certificate ----
Signed Authorities : ...
Transaction Kind : Publish
Gas Budget : 30000
----- Publish Results ----
The newly published package object: (A613A7FF8CB03E0DFC0D157E232BBA50C5F19D17, SequenceNumber(1), o#fb70b9d1ac25250a00e35031289f89cd9c9d739f8663e1cc8ed739095a104e68)
List of objects created by running module initializers: []
Updated Gas : Coin { id: 38B89FE9F4A4823F1406938E87A8767CBD7F0B93, value: 92939 }
```

As we can see, the package was successfully published. Some gas was charged: the initial gas value was 100000, now it's 92939 (note: the current gas charging mechanism is rather arbitrary, we will come up with a gas mechanism shortly). The newly published package has the ID `A613A7FF8CB03E0DFC0D157E232BBA50C5F19D17`. Note that this ID will also be different in your terminal. We add the package to another environment variable:

```
export PACKAGE=A613A7FF8CB03E0DFC0D157E232BBA50C5F19D17
```

### Playing TicTacToe <a href="#playing-tictactoe" id="playing-tictactoe"></a>

As we mentioned earlier, we need 3 parties to participate in this game: Admin, PlayerX and PlayerO. At a high level, the game works as following:

1. The admin creates a game, which also specifies the addresses of the two players. This will also create two capability objects and send to each of the addresses to give them permission to participate in the same game. This ensures that an arbitrary person cannot attempt to join this game.
2. Each player takes turns to send a _Mark_ object to the admin indicating where they want to place their mark.
3. The admin, upon receiving marks (in practice, this is done through monitoring events), places the mark to the gameboard.
4. (2) and (3) repeats until game ends.

Because the admin owns the gameboard, each individual player cannot place a mark directly on the gameboard (they don't own the object, and hence cannot mutate it, see Object Model), each mark placement is split to 2 steps, that each player first sends a mark, and then the admin places the mark. A sample gameplay can also be found in the [tic\_tac\_toe\_tests](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/games/tests/tic\_tac\_toe\_tests.move) file.

Now let's begin the game! First of all, let's create a game with the command:

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function create_game --args $PLAYER_X $PLAYER_O --gas $ADMIN_GAS --gas-budget 1000
```

You will see output like:

```
----- Certificate ----
Signed Authorities : ...
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17
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

The preceding call created three objects. For each object, it printed out a tuple of three values (object\_id, version, object\_digest). Object ID is what we care about here. Since we don't have a real application here to display things for us, we need a bit of object printing magic to figure out which object is which. Let's print out the metadata of each created object (replace the object ID with what you see on your screen):

```
$ sui client object 5851B7EA07B93E68696BC0CF811D2E266DFB880D
Owner: AddressOwner(k#251cf224b6ba3a019d04b6041357c20490f7a322)
Version: 1
ID: 5851B7EA07B93E68696BC0CF811D2E266DFB880D
Readonly: false
Type: 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17::TicTacToe::MarkMintCap

$ sui client object A6D3B507D4533822E690291166891D42694A2721
Owner: AddressOwner(k#7b61da6aaced7f28c1187d998955f10464beae55)
Version: 1
ID: A6D3B507D4533822E690291166891D42694A2721
Readonly: false
Type: 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17::TicTacToe::MarkMintCap

$ sui client object F1B8161BD97D3CD6627E739AD675089C5ACFB452
Owner: AddressOwner(k#ecf53ce22d1b2fb588573924057e9addad1d8385)
Version: 1
ID: F1B8161BD97D3CD6627E739AD675089C5ACFB452
Readonly: false
Type: 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17::TicTacToe::TicTacToe
```

There are two MarkMintCap objects (for capability of minting a mark for each player) and a TicTacToe object (the game object). Take a look at each of the `Owner` fields, and you will see that:

1. MarkMintCap Object `5851B7EA07B93E68696BC0CF811D2E266DFB880D` is owned by PLAYER\_O.
2. MarkMintCap Object `A6D3B507D4533822E690291166891D42694A2721` is owned by PLAYER\_X.
3. TicTacToe Object `F1B8161BD97D3CD6627E739AD675089C5ACFB452` is owned by ADMIN.

We add the above three object IDs to these environment variables:

```
$ export XCAP=A6D3B507D4533822E690291166891D42694A2721
export OCAP=5851B7EA07B93E68696BC0CF811D2E266DFB880D
export GAME=F1B8161BD97D3CD6627E739AD675089C5ACFB452
```

By convention, PlayerX goes first. PlayerX wants to put a mark at the center of the gameboard ((1, 1)). This needs to take two steps. First PlayerX creates a Mark object with the placement intention and send it to the admin. We will call the `send_mark_to_game` function in `TicTacToe`, whose signature looks like this:

```
public entry fun send_mark_to_game(cap: &mut MarkMintCap, game_address: address, row: u64, col: u64, ctx: &mut TxContext);
```

The `cap` argument will be PlayerX's capability object (XCAP), and `game_address` argument will be the admin's address (ADMIN):

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $XCAP $ADMIN 1 1 --gas $X_GAS --gas-budget 1000
```

And its output:

```
----- Certificate ----
Signed Authorities : ...
Transaction Kind : Call
Gas Budget : 1000
Package ID : 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17
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

The above call created a Mark object, with ID `AE3CE9176F1A8C1F21D922722486DF667FA00394`, and it was sent to the admin. The admin can now place the mark on the gameboard. The function to place the mark looks like this:

```
public entry fun place_mark(game: &mut TicTacToe, mark: Mark, ctx: &mut TxContext);
```

The first argument is the game board, and the second argument is the mark the admin just received from the player. We will call this function (replace the second argument with the Mark object ID above):

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0xAE3CE9176F1A8C1F21D922722486DF667FA00394 --gas $ADMIN_GAS --gas-budget 1000
```

The gameboard now looks like this (this won't be printed out, so keep it in your imagination):

PlayerO now tries to put a mark at (0, 0):

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $OCAP $ADMIN 0 0 --gas $O_GAS --gas-budget 1000
```

With output like:

```
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 102 }
Created Objects:
7A16D266DAD41145F34649258BC1F744D147BF2F SequenceNumber(1) o#58cb018be98dd828c10f5b2045329f6ec4dab56c5a90e719ad225f0bc195908a
...
```

Note, in this second call, the second argument comes from the created objects in the first call.

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0x7A16D266DAD41145F34649258BC1F744D147BF2F --gas $ADMIN_GAS --gas-budget 1000
```

With output like:

```
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 679 }
...
```

The gameboard now looks like this:

PlayerX puts a mark at (0, 2):

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $XCAP $ADMIN 0 2 --gas $X_GAS --gas-budget 1000
```

With output like:

```
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 102 }
Created Objects:
2875D50BD9021ED2009A1278C7CB6D4C876FFF6A SequenceNumber(1) o#d4371b72e77bfc07bd088a9113ef7bf870198066649f6c9e9e4abf5f7a7fbd2a
...
```

Then run:

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0x2875D50BD9021ED2009A1278C7CB6D4C876FFF6A --gas $ADMIN_GAS --gas-budget 1000
```

The gameboard now looks like this:

PlayerO places a mark at (1, 0):

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $OCAP $ADMIN 1 0 --gas $O_GAS --gas-budget 1000
```

With output like:

```
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 102 }
Created Objects:
4F7391F172063D87013DD9DC95B8BD45C35FD2D9 SequenceNumber(1) o#ee7ba8ea66e574d7636e159da9f33c96d724d71f141a57a1e4929b2b928c298d
...
```

Now run:

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0x4F7391F172063D87013DD9DC95B8BD45C35FD2D9 --gas $ADMIN_GAS --gas-budget 1000
...
```

The gameboard now looks like:

This is a chance for PlayerX to win! X now mints the winning mark at (2, 0):

```
$ sui client call --package $PACKAGE --module tic_tac_toe --function send_mark_to_game --args $XCAP $ADMIN 2 0 --gas $X_GAS --gas-budget 1000
```

And its output:

```
----- Certificate ----
...
----- Transaction Effects ----
Status : Success { gas_used: 102 }
Created Objects:
AA7A6624E16E5E447801462FF6614013FC4AD156 SequenceNumber(1) o#e5e1b15f03531db118efaa9667244b876f32e7ad2cc17bdbc7d4cb1eaca1560d
...
```

And then finally the admin places the winning mark:

```
   $ sui client call --package $PACKAGE --module tic_tac_toe --function place_mark --args $GAME 0xAA7A6624E16E5E447801462FF6614013FC4AD156 --gas $ADMIN_GAS --gas-budget 1000
```

With output:

```
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

```
$ sui client object 54B58C0D5B14A269B1CD424B3CCAB1E315C43343
```

See output resembling:

```
Owner: AddressOwner(k#7b61da6aaced7f28c1187d998955f10464beae55)
Version: 1
ID: 54B58C0D5B14A269B1CD424B3CCAB1E315C43343
Readonly: false
Type: 0xa613a7ff8cb03e0dfc0d157e232bba50c5f19d17::TicTacToe::Trophy
```

PlayerX has received a Trophy object and hence won the game!

This concludes the tutorial.
