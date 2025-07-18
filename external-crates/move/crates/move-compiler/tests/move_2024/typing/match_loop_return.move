module a::m;

#[allow(unused_let_mut)]
fun test(): bool {
    let values = 'search: {
        vector[1,2,3u8].destroy!(|v| {
            match (v) {
                255 => return 'search false,
                0 => return 'search true,
                _ => return 'search true,
            }
        });
        true
    };
    values
}
