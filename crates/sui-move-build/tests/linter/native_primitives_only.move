
module 0x42::test1{
    use std::vector;
    //has copy needed because it's a vector

    struct S1 has copy{}
    
    struct Coolstruct has copy,drop{
        a: bool,
        b: u64,
    }

    public entry fun main(){
        let v = vector::empty<Coolstruct>();
        //The linter should complain because push_back is native 
        //and S2 is not a primitive type
        vector::push_back(&mut v, Coolstruct{a:true,b:42});
    }
}
