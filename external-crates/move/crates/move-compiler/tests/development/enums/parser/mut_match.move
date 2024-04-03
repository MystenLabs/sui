module 0x42::m {
    public enum Option<T> {
      Some(T),
      Other { x: T },
      None
    }

    public fun weird_is_some(o: Option<bool>): bool {
       match (o) {
         Option::Some(true) => true,
         Option::Other { mut x } => x,
         Option::Other { x: mut y } => y,
         Option::Some(mut x) => {
             x = true && x;
             x
         },
         Option::None => false,
       }
    }
}
