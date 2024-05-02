module 0x42::m {
    public enum Option<T> {
      Some(T),
      Other { x: T },
      None
    }

    public fun weird_is_some(o: Option<bool>): bool {
       match (o) {
         Option::Other { mut x: mut y } => y,
         Option::Other { x: mut y<u64> } => y,
         Option::Other { mut x: y } => y,
         x @ mut Option::Some(true) => true,
         mut Option::None => false,
       }
    }
}
