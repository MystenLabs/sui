export const convertNumberToDate = (epochTime: number) => {
    const date = new Date(epochTime);

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
    } ${date.getUTCFullYear()} ${date.getUTCHours()}:${date.getUTCMinutes()}:${date.getSeconds()} (UTC)`;
};
