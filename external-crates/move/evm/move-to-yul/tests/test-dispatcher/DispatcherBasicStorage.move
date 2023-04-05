#[evm_contract]
module 0x2::M {
    #[storage]
    struct Storage {
        counter: u64,
    }

    #[create]
    public fun create(): Storage {
        Storage{counter: 0}
    }

    #[callable]
    fun current(self: &Storage): u64 {
        self.counter
	}

    #[callable]
    fun increment(self: &mut Storage) {
	    self.counter = self.counter + 1;
    }

    #[receive, payable]
    fun receive(self: &mut Storage) {
        self.counter = self.counter + 2;
    }

    #[fallback]
    fun fallback(_self: &mut Storage) {
    }
}
