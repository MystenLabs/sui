module 0x42::t {

public struct Box<T> { value: T }

#[syntax(index)]
public fun in<T: drop>(self: &Box<T>): &T { &self.value }

#[syntax(index)]
public fun in_mut<T: copy>(self: &mut Box<T>): &mut T { &mut self.value }

// invalid
public fun test00<A>(b: &Box<A>) {
    let b_val = b[];
}

// ok
public fun test01<A: drop>(b: &Box<A>) {
    let b_val = &b[];
}

// invalid
public fun test02<A>(b: &mut Box<A>) {
    let b_val = &b[];
}

// invalid
public fun test03<A: drop>(b: &mut Box<A>) {
    let b_val = &mut b[];
}

// ok
public fun test04<C: copy>(b: &mut Box<A>) {
    let b_val = copy &mut b[];
}

public fun test05<A: copy,B: drop>(b: &Box<A>, mb: &Box<B>) {
    let b_val = &b[]; // invalid
    let mb_val = &mut mb[]; // invalid
}

public fun test06<A:drop ,B: copy>(b: &Box<A>, mb: &Box<B>) {
    let b_val = &b[];
    let q = copy b_val; // invalid
    let mb_val = &mut mb[];
    let r = copy mb_val; // valid
    // invalid -- can't drop mb_val
}

}
