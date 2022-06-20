const stdToTwo = (value: number) => (value < 10 ? `0${value}` : `${value}`);

export const convertNumberToDate = (epochTimeSecs: number | null): string => {
    if (!epochTimeSecs) return 'Not Available';
    //API returns epoch time in seconds, Date expects milliseconds:
    const date = new Date(epochTimeSecs * 1000);

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

    return `${date.getUTCDate()} ${
        MONTHS[date.getUTCMonth()]
    } ${date.getUTCFullYear()} ${stdToTwo(date.getUTCHours())}:${stdToTwo(
        date.getUTCMinutes()
    )}:${stdToTwo(date.getSeconds())} (UTC)`;
};
