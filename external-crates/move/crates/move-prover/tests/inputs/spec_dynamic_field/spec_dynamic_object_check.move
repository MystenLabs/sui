module 0x42::dynamic_fields {
    use sui::dynamic_object_field as ofield;
    use std::u128;

    public struct Parent has key {
        id: UID,
    }

    public struct DFOChild has key, store {
        id: UID,
        count: u64
    }

    fun sqrt(x: u128, ctx: &mut TxContext): u64 {
        u128::sqrt(x) as u64
    }

    #[spec_only]
    use prover::prover::ensures;

    #[spec(prove)]
    fun sqrt_spec(x: u128, ctx: &mut TxContext): u64 {
        let x_int = x.to_int();

        let result = sqrt(x, ctx);

        let result_int = result.to_int();

        let parent_id = object::new(ctx);
        let mut parent = Parent { id: parent_id };
        let mut parent_ref = &mut parent;

        let dof_child_id = object::new(ctx);
        let dfo_child = DFOChild { id: dof_child_id, count: 42 };
        let dfo_name = b"dfochild";

        ofield::add(&mut parent_ref.id, dfo_name, dfo_child);

        ensures(result_int.mul(result_int).lte(x_int));
        ensures(result_int.add(1u64.to_int()).mul(result_int.add(1u64.to_int())).gt(x_int));
        
        transfer::share_object(parent);

        result
    }
}
