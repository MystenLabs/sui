module 0x42::foo;

use prover::prover::ensures;
use prover::ghost;

public struct GhostStruct {}


#[spec(prove)]
fun ghost_borrow_mut_spec() {
  ghost::declare_global_mut<GhostStruct, bool>();
  let ghost_ref = ghost::borrow_mut<GhostStruct, bool>();
  *ghost_ref = true;
  *ghost::global<GhostStruct, bool>();
  ensures(ghost::global<GhostStruct, bool>() == true);
}
