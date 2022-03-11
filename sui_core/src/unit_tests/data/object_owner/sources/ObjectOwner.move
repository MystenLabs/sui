module ObjectOwner::ObjectOwner {
    use Std::Option::{Self, Option};
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer::{Self, ChildRef};
    use Sui::TxContext::{Self, TxContext};

    struct Parent has key {
        id: VersionedID,
        child: Option<ChildRef<Child>>,
    }

    struct Child has key {
        id: VersionedID,
    }

    public fun create_child(ctx: &mut TxContext) {
        Transfer::transfer(
            Child { id: TxContext::new_id(ctx) },
            TxContext::sender(ctx),
        );
    }

    public fun create_parent(ctx: &mut TxContext) {
        Transfer::transfer(
            Parent { id: TxContext::new_id(ctx), child: Option::none() },
            TxContext::sender(ctx),
        );
    }

    public fun create_parent_and_child(ctx: &mut TxContext) {
        let parent_id = TxContext::new_id(ctx);
        let child = Child { id: TxContext::new_id(ctx) };
        let (parent_id, child_ref) = Transfer::transfer_to_object_id(child, parent_id);
        let parent = Parent {
            id: parent_id,
            child: Option::some(child_ref),
        };
        Transfer::transfer(parent, TxContext::sender(ctx));
    }

    public fun add_child(parent: &mut Parent, child: Child, _ctx: &mut TxContext) {
        let child_ref = Transfer::transfer_to_object(child, parent);
        Option::fill(&mut parent.child, child_ref);
    }

    // Call to mutate_child will fail if its owned by a parent,
    // since all owners must be in the arguments for authentication.
    public fun mutate_child(_child: &mut Child, _ctx: &mut TxContext) {}

    // This should always succeeds, even when child is not owned by parent.
    public fun mutate_child_with_parent(_child: &mut Child, _parent: &mut Parent, _ctx: &mut TxContext) {}

    public fun transfer_child(parent: &mut Parent, child: Child, new_parent: &mut Parent, _ctx: &mut TxContext) {
        let child_ref = Option::extract(&mut parent.child);
        let new_child_ref = Transfer::transfer_child_to_object(child, child_ref, new_parent);
        Option::fill(&mut new_parent.child, new_child_ref);
    }

    public fun remove_child(parent: &mut Parent, child: Child, ctx: &mut TxContext) {
        let child_ref = Option::extract(&mut parent.child);
        Transfer::transfer_child_to_address(child, child_ref, TxContext::sender(ctx));
    }

    // Call to delete_child can fail if it's still owned by a parent.
    public fun delete_child(child: Child, _parent: &mut Parent, _ctx: &mut TxContext) {
        let Child { id } = child;
        ID::delete(id);
    }

    public fun delete_parent_and_child(parent: Parent, child: Child, _ctx: &mut TxContext) {
        let Parent { id, child: child_ref_opt } = parent;
        let child_ref = Option::extract(&mut child_ref_opt);
        Option::destroy_none(child_ref_opt);
        Transfer::delete_child_object(child, child_ref);
        ID::delete(id);
    }
}
