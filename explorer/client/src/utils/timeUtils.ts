const stdToN = (original: number, length: number) =>
    String(original).padStart(length, '0');

export const convertNumberToDate = (epochTimeSecs: number | null): string => {
    if (!epochTimeSecs) return 'Not Available';
    //API returns epoch time in seconds, Date expects milliseconds:
    const date = new Date(epochTimeSecs);

    const MONTHS = [
        'Jan',
        'Feb',
        'Mar',
        'Apr',
        'May',
        'Jun',
        'Jul',
        'Aug',
        'Sep',
        'Oct',
        'Nov',
        'Dec',
    ];

    return `${stdToN(date.getUTCDate(), 2)} ${
        MONTHS[date.getUTCMonth()]
    } ${date.getUTCFullYear()} ${stdToN(date.getUTCHours(), 2)}:${stdToN(
        date.getUTCMinutes(),
        2
    )}:${stdToN(date.getUTCSeconds(), 2)}.${stdToN(
        date.getUTCMilliseconds(),
        3
    )} (UTC)`;
};
