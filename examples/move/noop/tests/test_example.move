#[test_only]
module noop::example_test {
    use sui::test_scenario::{Self};
    use noop::example::{Self};
    use sui::transfer::{Self};

    // Test address
    const USER: address = @0xCAFE;

    #[test]
    fun noop() {
        let scenario_val = test_scenario::begin(USER);
        let scenario = &mut scenario_val;
        

        test_scenario::next_tx(scenario, USER);
        {
            example::noop();
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun noop_w_metadata() {
        let scenario_val = test_scenario::begin(USER);
        let scenario = &mut scenario_val;
        

        test_scenario::next_tx(scenario, USER);
        {
            example::noop_w_metadata(vector[1, 2]);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun noop_w_metadata_event() {
        let scenario_val = test_scenario::begin(USER);
        let scenario = &mut scenario_val;
        

        test_scenario::next_tx(scenario, USER);
        {
            let ctx = test_scenario::ctx(scenario);

            example::noop_w_metadata_event(vector[1, 2], ctx);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun add_metadata() {
        let scenario_val = test_scenario::begin(USER);
        let scenario = &mut scenario_val;
        

        test_scenario::next_tx(scenario, USER);
        {
            let ctx = test_scenario::ctx(scenario);
            
            let metadata = example::add_metadata(vector[1, 2], ctx);

            transfer::public_transfer(metadata, USER);
        };

        test_scenario::end(scenario_val);
    }
}
