export const PACKAGE_ID = '0x7e03e23a451db32eb70b094a38c447f7d3058788';
export const MODULE = 'lucky_capy';
export const TYPE_LOTTERY = `${PACKAGE_ID}::${MODULE}::LuckyCapyLottery`;
export const STATUS_INITIALIZED = 0;
export const STATUS_RUNNING = 1;
export const STATUS_ENDED = 2;
export const STATUS_TO_TXT = {
    [STATUS_INITIALIZED]: 'Initialized',
    [STATUS_RUNNING]: 'Running',
    [STATUS_ENDED]: 'Ended',
};
