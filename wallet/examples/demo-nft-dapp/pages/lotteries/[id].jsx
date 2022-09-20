import { useRouter } from 'next/router';
import { useRef } from 'react';
import { useCallback } from 'react';
import { useMemo } from 'react';
import {
    MODULE,
    PACKAGE_ID,
    STATUS_ENDED,
    STATUS_INITIALIZED,
    STATUS_RUNNING,
    STATUS_TO_TXT,
} from '../../lottery/constants';
import { useAPI } from '../../shared/api-context';
import { useSuiObjects } from '../../shared/objects-store-context';
import Link from 'next/link';

const prizes = [
    {
        prize: 'Pillow',
        won(position, totalCapys, wonPrizes) {
            return position <= 2;
        },
    },
    {
        prize: 'Amazon card #1',
        won(position, totalCapys, wonPrizes) {
            return (
                (position > 2 && position <= 6) ||
                (totalCapys <= position + 1 && wonPrizes.length === 0)
            );
        },
    },
    {
        prize: 'Amazon card #2',
        won(position, totalCapys, wonPrizes) {
            return position > 6 && position <= 11;
        },
    },
    {
        prize: 'stuffed animal',
        won(position, totalCapys, wonPrizes) {
            return true;
        },
    },
];

function getPrize(position, totalCapys) {
    const wonPrizes = [];
    for (const aPrize of prizes) {
        if (aPrize.won(position, totalCapys, wonPrizes)) {
            wonPrizes.push(aPrize.prize);
        }
    }
    return wonPrizes.join(' + ') || null;
}

const LotteryPage = () => {
    const router = useRouter();
    const txtRef = useRef();
    const { id } = router.query;
    const { suiObjects } = useSuiObjects();
    const lottery = useMemo(() => suiObjects[id] || null, [suiObjects, id]);
    const existingCapys = useMemo(
        () =>
            (lottery?.data?.fields?.capys || [])
                .map((aCapy) => aCapy.fields)
                .sort((a, b) => {
                    const s = b.score - a.score;
                    if (s === 0) {
                        return a.name.localeCompare(b.name);
                    }
                    return s;
                }),
        [lottery]
    );
    const api = useAPI();
    const onHandleAdd = useCallback(async () => {
        const namesStr = txtRef?.current?.value || '';
        const names = namesStr
            .split('\n')
            .map((aName) => aName.trim())
            .filter(
                (aName) =>
                    !!aName &&
                    !existingCapys.find((aCapy) => aName === aCapy.name)
            );
        if (names.length && api) {
            try {
                for (const aName of names) {
                    await suiWallet.executeMoveCall({
                        packageObjectId: PACKAGE_ID,
                        module: MODULE,
                        function: 'add_capy',
                        typeArguments: [],
                        arguments: [id, aName],
                        gasBudget: 1000000,
                    });
                }
                if (txtRef.current) {
                    txtRef.current.value = '';
                }
            } catch (e) {
                console.error(e);
            }
        }
    }, [api, existingCapys]);
    const rollRef = useRef();
    const onHandleRoll = useCallback(async () => {
        if (!lottery) {
            return;
        }
        const rollsStr = rollRef?.current?.value || '';
        const rolls = rollsStr
            .split('\n')
            .map((val) => val.trim())
            .map((val) => val.split('-'))
            .map(([aName, ...aSeed]) => [aName.trim(), aSeed.join('-').trim()])
            .filter(([aName, aSeed]) => aName && aSeed);
        try {
            for (const [aName, aSeed] of rolls) {
                await suiWallet.executeMoveCall({
                    packageObjectId: PACKAGE_ID,
                    module: MODULE,
                    function: 'roll_dice',
                    typeArguments: [],
                    arguments: [id, aName, aSeed],
                    gasBudget: 1000000,
                });
            }
            if (rolls.length && rollRef.current) {
                rollRef.current.value = '';
            }
        } catch (e) {
            console.error(e);
        }
    }, [lottery]);
    const onHandleEnd = useCallback(async () => {
        try {
            await suiWallet.executeMoveCall({
                packageObjectId: PACKAGE_ID,
                module: MODULE,
                function: 'end_lottery',
                typeArguments: [],
                arguments: [id],
                gasBudget: 1000000,
            });
        } catch (e) {
            console.error(e);
        }
    }, []);
    if (!lottery) {
        return (
            <h5>
                Lottery <b>{id}</b> not found or loading.
            </h5>
        );
    }
    const {
        reference: { objectId },
        data: {
            fields: { status, round },
        },
    } = lottery;

    return (
        <div>
            <h5>
                Lottery {objectId} ({STATUS_TO_TXT[status]})
                <Link href={`/graphs/${id}`}>📊</Link>
            </h5>
            {status === STATUS_INITIALIZED ? (
                <section>
                    <hr />
                    <h6>Add capy</h6>
                    <div>
                        <textarea
                            rows="5"
                            cols="40"
                            ref={txtRef}
                            placeholder="Capy name | can add multiple in each line"
                        />
                    </div>
                    <button type="button" onClick={onHandleAdd}>
                        Add
                    </button>
                    <hr />
                </section>
            ) : null}
            {status !== STATUS_ENDED ? (
                <section>
                    <h6>Roll dice</h6>
                    <div>
                        <textarea
                            rows="5"
                            cols="40"
                            ref={rollRef}
                            placeholder="Roll info name - info | can add multiple in each line"
                        />
                    </div>
                    <button type="button" onClick={onHandleRoll}>
                        Roll
                    </button>
                    <hr />
                </section>
            ) : null}
            {status === STATUS_RUNNING && round > 0 ? (
                <section>
                    <button type="button" onClick={onHandleEnd}>
                        End Lottery
                    </button>
                </section>
            ) : null}
            <section>
                <h4>
                    Round {round} | Capys: {existingCapys.length}
                </h4>
                <ol>
                    {existingCapys.map(
                        ({ name, score, id: { id: aCapyID } }, i) => (
                            <li key={aCapyID}>
                                <b>{name}</b> ({score})
                                {status === STATUS_ENDED ? (
                                    <> | {getPrize(i, existingCapys.length)}</>
                                ) : null}
                            </li>
                        )
                    )}
                </ol>
                {status === STATUS_ENDED && existingCapys.length ? (
                    <h1 style={{ textAlign: 'center' }}>
                        Winner {existingCapys[0].name} 🎉🥳🥳
                    </h1>
                ) : null}
            </section>
        </div>
    );
};

export default LotteryPage;
