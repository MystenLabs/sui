import { Transaction, Commands } from './';

const tx = new Transaction({ inputs: ['amount', 'address'] });
const coin = tx.add(Commands.Split(tx.gas(), tx.input('amount')));

// NOTE: Do this:
// Option 1, coin is directly usable
tx.add(Commands.TransferObjects([coin], tx.input('address')));
// This also allows us to destructure out variable returns:
const [gameNFT, gameAccount] = tx.add(
  Commands.MoveCall({
    package: '0x2',
    function: 'game',
    module: 'register',
    arguments: [],
    typeArguments: [],
  }),
);

// Option 2, `.result()` method:
tx.add(Commands.TransferObjects([coin.result()], tx.input('address')));
// This requires calling result with an index:
const register = tx.add(
  Commands.MoveCall({
    package: '0x2',
    function: 'game',
    module: 'register',
    arguments: [],
    typeArguments: [],
  }),
);
register.result(0);
register.result(1);

console.log(tx);

const serialized = tx.serialize();
console.log(serialized);

const tx2 = Transaction.from(serialized);
console.log(tx2);
