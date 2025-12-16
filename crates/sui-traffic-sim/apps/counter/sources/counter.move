module counter::counter {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// A Counter object that holds an integer value
    public struct Counter has key, store {
        id: UID,
        value: u64
    }

    /// Create a new counter object with initial value of 0
    public entry fun create(ctx: &mut TxContext) {
        let counter = Counter {
            id: object::new(ctx),
            value: 0
        };
        transfer::public_share_object(counter);
    }

    /// Increment the counter by 1
    public entry fun increment(counter: &mut Counter) {
        counter.value = counter.value + 1;
    }

    /// Get the current value of the counter (for reading/testing)
    public fun value(counter: &Counter): u64 {
        counter.value
    }

    #[test_only]
    use sui::test_scenario;

    #[test]
    fun test_counter() {
        let user = @0x1;
        let mut scenario = test_scenario::begin(user);

        // Create a counter
        {
            let ctx = test_scenario::ctx(&mut scenario);
            create(ctx);
        };

        // Increment the counter
        test_scenario::next_tx(&mut scenario, user);
        {
            let mut counter = test_scenario::take_shared<Counter>(&scenario);
            assert!(value(&counter) == 0, 0);

            increment(&mut counter);
            assert!(value(&counter) == 1, 1);

            increment(&mut counter);
            assert!(value(&counter) == 2, 2);

            test_scenario::return_shared(counter);
        };

        test_scenario::end(scenario);
    }
}