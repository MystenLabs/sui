module a::m;

public struct X { x: u64 }

fun f(cond: bool) {
   let mut _y;
   let mut _x = X { x: 0 };
   _y = X { x: 0 };
   if (cond) {
      let X { x: _ } = _x;
   } else {
      let X { x: _ } = _x;
   };
   if (cond) {
      let X { x: _ } = _y;
   } else {
      let X { x: _ } = _y;
   };
}
