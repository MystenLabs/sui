#[evm_contract]
module 0x2::M {
    use Evm::Evm::{emit};
    use Evm::U256::U256;

    // No topics


    #[event(sig=b"Transfer(address,address,uint256)")]
    struct Transfer {
        from: address,
        to: address,
        value: U256,
    }

    #[callable]
    fun do_transfer(from: address, to: address, amount: U256){
        emit(Transfer{from, to, value: amount});
    }


    #[event(sig=b"Event(uint8,address,uint256)")]
    struct Event_1 {
        from: u8,
        to: address,
        value: U256,
    }

    #[callable]
    fun do_event_1(from: u8, to: address, amount: U256){
        emit(Event_1{from, to, value: amount});
    }


    #[event(sig=b"Event(uint8,address,uint16,bytes)")]
    struct Event_2 {
        v1: u8,
        v2: address,
        v3: u64,
        v4: vector<u8>
    }

    #[callable(sig=b"ev(uint8,address,uint16,bytes)")]
    fun do_event_2(v1: u8, v2: address, v3: u64, v4: vector<u8>){
        emit(Event_2{v1, v2, v3, v4});
    }

    // Topics with value type

    #[event(sig=b"Event(uint8 indexed,address,uint256 indexed)")]
    struct Event_3 {
        from: u8,
        to: address,
        value: U256,
    }

    #[callable]
    fun do_event_3(from: u8, to: address, amount: U256){
        emit(Event_3{from, to, value: amount});
    }

    #[event(sig=b"Event(bytes1 indexed,bytes2 indexed,bytes32 indexed)")]
    struct Event_4 {
        v1: vector<u8>,
        v2: vector<u8>,
        v3: vector<u8>,
    }

    #[callable(sig=b"ev(bytes1,bytes2,bytes32)")]
    fun do_event_4(v1: vector<u8>, v2: vector<u8>, v3: vector<u8>){
        emit(Event_4{v1, v2, v3});
    }

    // Topics with non-value type

    #[event(sig=b"Event(bytes indexed, string indexed)")]
    struct Event_5 {
        bys: vector<u8>,
        str: vector<u8>
    }

    #[callable(sig=b"ev(bytes, string)")]
    fun do_event_5(bys: vector<u8>, str: vector<u8>){
        emit(Event_5{bys, str});
    }

    #[event(sig=b"Event(bytes indexed, string, uint16[3] indexed)")]
    struct Event_6 {
        bys: vector<u8>,
        str: vector<u8>,
        uint16_array: vector<u64>
    }

    #[callable(sig=b"ev(bytes , string , uint16[3] )")]
    fun do_event_6(bys: vector<u8>, str: vector<u8>, uint16_array: vector<u64>){
        emit(Event_6{bys, str, uint16_array});
    }

    #[event(sig=b"Event(bytes[] indexed)")]
    struct Event_7 {
        bys: vector<vector<u8>>,
    }

    #[callable(sig=b"ev(bytes[])")]
    fun do_event_7(bys: vector<vector<u8>>){
        emit(Event_7{bys});
    }

    #[event(sig=b"Event(bytes[], string[3] indexed)")]
    struct Event_8 {
        bys: vector<vector<u8>>,
        strs: vector<vector<u8>>
    }

    #[callable(sig=b"ev(bytes[], string[3])")]
    fun do_event_8(bys: vector<vector<u8>>, strs: vector<vector<u8>>){
        emit(Event_8{bys, strs});
    }
}
