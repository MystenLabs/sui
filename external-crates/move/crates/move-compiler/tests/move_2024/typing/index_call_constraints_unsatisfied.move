module 0x42::t {

public struct Box<T> { value: T }

#[syntax(index)]
public fun in<T: drop + copy>(self: &Box<T>): &T { &self.value }

#[syntax(index)]
public fun in_mut<T: drop + copy>(self: &mut Box<T>): &mut T { &mut self.value }

// ok
public fun test00<A: drop + copy>(b: &Box<A>) {
    let _b_val = &b[];
}

// invalid
public fun test01<A>(b: &Box<A>) {
    let _b_val = b[];
}

// invalid
public fun test02<A>(b: &mut Box<A>) {
    let _b_val = &b[];
}

// invalid
public fun test03<A: drop>(b: &mut Box<A>) {
    let _b_val = &mut b[];
}

// invalid
public fun test04<A: copy>(_b: &mut Box<A>) {
    let _b_val = copy &mut _b[];
}

// invalid
public fun test05<A: copy,B: drop>(b: &Box<A>, mb: &mut Box<B>) {
    let _b_val = &b[]; // invalid
    let _mb_val = &mut mb[]; // invalid
}

// invalid
public fun test06<A:drop ,B: copy>(b: &Box<A>, mb: &mut Box<B>) {
    let _b_val = &b[];
    let _mb_val = &mut mb[];
}

}
