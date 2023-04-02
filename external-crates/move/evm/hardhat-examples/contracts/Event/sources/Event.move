#[evm_contract]
module 0x1::FortyTwo {
    use Evm::Evm::{emit};
    use Evm::U256::U256;

    #[event]
    struct SimpleEvent {
        x: u64,
    }

    #[callable(sig=b"emitNothing(uint64)")]
    public fun emitNothing(_x: u64) {
    }

    #[callable(sig=b"emitSimpleEvent(uint64)")]
    public fun emitSimpleEvent(x: u64) {
        emit(SimpleEvent{x});
    }

    #[callable(sig=b"emitSimpleEventTwice(uint64)")]
    public fun emitSimpleEventTwice(x: u64) {
        emit(SimpleEvent{x});
        emit(SimpleEvent{x: x+x});
    }

    #[event(sig=b"MyEvent(uint64,string)")]
    struct MyEvent {
        x: u64,
        message: vector<u8>,
    }

    #[callable(sig=b"emitMyEvent(uint64)")]
    public fun emitMyEvent(x: u64) {
        emit(MyEvent{x, message: b"hello_event"});
    }

    #[callable(sig=b"emitMyEventTwice(uint64)")]
    public fun emitMyEventTwice(x: u64) {
        emit(MyEvent{x, message: b"hello_event_#1"});
        emit(MyEvent{x: x+x, message: b"hello_event_#2"});
    }

    #[callable(sig=b"emitMyEventWith(uint64,string)")]
    public fun emitMyEventWith(x: u64, message: vector<u8>) {
        emit(MyEvent{x, message});
    }

    #[callable(sig=b"emitMyEventWithTwice(uint64,string)")]
    public fun emitMyEventWithTwice(x: u64, message: vector<u8>) {
        emit(MyEvent{x, message});
        emit(MyEvent{x: x+x, message});
    }

    #[event(sig=b"Transfer(address indexed,address indexed,uint256)")]
    struct Transfer {
        from: address,
        to: address,
        value: U256,
    }

    #[callable(sig=b"emitTransfer(address,address,uint256)")]
    public fun emitTransfer(from: address, to: address, value: U256) {
        emit(Transfer{from, to, value});
    }

    #[event(sig=b"U256Event(uint256)")]
    struct U256Event {
        x: U256,
    }

    #[callable(sig=b"emitU256Event(uint256)")]
    public fun emitU256Event(x: U256) {
        emit(U256Event{x});
    }

    #[event(sig=b"AddressEvent(address)")]
    struct AddressEvent {
        a: address,
    }

    #[callable(sig=b"emitAddressEvent(address)")]
    public fun emitAddressEvent(a: address) {
        emit(AddressEvent{a});
    }
}
