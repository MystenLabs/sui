module 0x42::m;

fun test() {
    'l: loop {
        'l: loop {
            break 'l
        }
    }
}
