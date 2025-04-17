module 0x42::dynamic_fields {
    use std::u128;
    use sui::event;

    public struct TestEvent has copy, drop, store {
        value: u128,
    }

    fun sqrt(x: u128): u64 {
        u128::sqrt(x) as u64
    }

    fun subcheck(x: u128) {
        event::emit(TestEvent { value: x });
    }

    #[spec_only]
    use prover::prover::ensures;

    #[spec(prove)]
    fun sqrt_spec(x: u128): u64 {
        let x_int = x.to_int();

        let result = sqrt(x);
        let result_int = result.to_int();

        ensures(result_int.mul(result_int).lte(x_int));
        ensures(result_int.add(1u64.to_int()).mul(result_int.add(1u64.to_int())).gt(x_int));
        
        subcheck(x);

        result
    }
}
