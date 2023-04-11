#[evm_contract]
module 0x2::ConstructorTest {
    use Evm::Evm::sign;

    struct Balance has key, drop {
        value: u64,
        value2: u64
    }

    #[create]
    fun init(value: u64, value2: u64) {
        move_to(&sign(@0x42), Balance { value, value2});
    }

    #[callable]
    fun test(): u64 acquires Balance {
        borrow_global<Balance>(@0x42).value
    }
}
