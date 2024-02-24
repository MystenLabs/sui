module 0x42::t {

public struct Box<T> { value: T }

#[syntax(index)]
public fun in<T: drop>(self: &Box<T>): &T { &self.value }

#[syntax(index)]
public fun in_mut<T: copy>(self: &mut Box<T>): &mut T { &mut self.value }

public struct Box2<T> { value: T }

#[syntax(index)]
public fun in2<T: drop + copy + store>(self: &Box2<T>): &T { &self.value }

#[syntax(index)]
public fun in_mut2<T: copy + drop>(self: &mut Box2<T>): &mut T { &mut self.value }

}
