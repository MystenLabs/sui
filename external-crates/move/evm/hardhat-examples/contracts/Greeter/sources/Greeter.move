#[evm_contract]
module Evm::Greeter {
    use Evm::Evm::{self};
    use Evm::Evm::sign;

    struct State has key {
        greeting: vector<u8>,
    }

    #[create(sig=b"constructor(string)")]
    public fun create(greeting: vector<u8>) {
         move_to<State>(
             &sign(self()),
             State {
                 greeting,
             }
         );
    }

    #[callable(sig=b"greet() returns (string)"), view]
    public fun greet(): vector<u8> acquires State {
        borrow_global<State>(self()).greeting
    }

    #[callable(sig=b"setGreeting(string)")]
    public fun setGreeting(greeting: vector<u8>) acquires State {
        borrow_global_mut<State>(self()).greeting = greeting;
    }
}
