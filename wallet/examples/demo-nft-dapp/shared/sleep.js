export function sleep(millis) {
    return new Promise((r) =>
        setTimeout(r, typeof millis !== undefined ? millis : 1000)
    );
}
