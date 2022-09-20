import { useRouter } from 'next/router';
import { useMemo } from 'react';
import { STATUS_ENDED, STATUS_TO_TXT } from '../../lottery/constants';
import { useSuiObjects } from '../../shared/objects-store-context';
import { useEffect } from 'react';
import * as d3 from 'd3';
import { useRef } from 'react';
import Link from 'next/link';

const margin = { top: 16, right: 6, bottom: 6, left: 0 };
const barSize = 40;
const width = 2000;
const x = d3.scaleLinear([0, 1], [margin.left, width - margin.right]);
function textTween(a, b) {
    const i = d3.interpolateNumber(a, b);
    return function (t) {
        this.textContent = formatNumber(i(t));
    };
}
const formatNumber = d3.format(',d');

const GraphPage = () => {
    const router = useRouter();
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
    const totalCapys = existingCapys.length;
    const { objectId, status, round } = useMemo(
        () =>
            (lottery && {
                objectId: lottery.reference.objectId,
                status: lottery.data.fields.status,
                round: lottery.data.fields.round,
            }) ||
            {},
        [lottery]
    );
    const graphNode = useRef();
    const graph = useRef();
    useEffect(() => {
        if (!graph.current && totalCapys && graphNode.current) {
            const height = margin.top + barSize * totalCapys + margin.bottom;
            const svg = d3.create('svg').attr('viewBox', [0, 0, width, height]);
            graphNode.current.append(svg.node());

            const n = totalCapys;
            const y = d3
                .scaleBand()
                .domain(d3.range(n + 1))
                .rangeRound([margin.top, margin.top + barSize * (n + 1 + 0.1)])
                .padding(0.1);

            const scale = d3.scaleOrdinal(d3.schemeTableau10);
            const color = (d) => scale(d.c.name);
            let bar = svg
                .append('g')
                .attr('fill-opacity', 0.6)
                .selectAll('rect');
            const updateBars = (data, transition) =>
                (bar = bar
                    .data(data.slice(0, n), (d) => d.c.name)
                    .join(
                        (enter) =>
                            enter
                                .append('rect')
                                .attr('fill', color)
                                .attr('height', y.bandwidth())
                                .attr('x', x(0))
                                .attr('y', (d) => y(((d.p && d.p) || d.c).rank))
                                .attr(
                                    'width',
                                    (d) => x(((d.p && d.p) || d.c).value) - x(0)
                                ),
                        (update) => update,
                        (exit) =>
                            exit
                                .transition(transition)
                                .remove()
                                .attr('y', (d) => y(d.c.rank))
                                .attr('width', (d) => x(d.c.value) - x(0))
                    )
                    .call((bar) =>
                        bar
                            .transition(transition)
                            .attr('y', (d) => y(d.c.rank))
                            .attr('width', (d) => x(d.c.value) - x(0))
                    ));
            const updateAxis = (function axis(theSvg) {
                const g = theSvg
                    .append('g')
                    .attr('transform', `translate(0,${margin.top})`);

                const axis = d3
                    .axisTop(x)
                    .ticks(width / 160)
                    .tickSizeOuter(0)
                    .tickSizeInner(-barSize * (n + y.padding()));

                return (_, transition) => {
                    g.transition(transition).call(axis);
                    g.select('.tick:first-of-type text').remove();
                    g.selectAll('.tick:not(:first-of-type) line').attr(
                        'stroke',
                        'white'
                    );
                    g.select('.domain').remove();
                };
            })(svg);
            function labels(theSvg) {
                let label = theSvg
                    .append('g')
                    .style('font-weight', 'bold')
                    .style(
                        'font-size',
                        `${Math.max(10, Math.round(barSize * 0.3))}px`
                    )
                    .style('font-variant-numeric', 'tabular-nums')
                    .attr('text-anchor', 'end')
                    .selectAll('text');

                return (data, transition) =>
                    (label = label
                        .data(data, (d) => d.c.name)
                        .join(
                            (enter) =>
                                enter
                                    .append('text')
                                    .attr(
                                        'transform',
                                        (d) =>
                                            `translate(${x(
                                                ((d.p && d.p) || d.c).value
                                            )},${y(
                                                ((d.p && d.p) || d.c).rank
                                            )})`
                                    )
                                    .attr('y', y.bandwidth() / 2)
                                    .attr('x', -6)
                                    .attr('dy', '-0.25em')
                                    .text((d) => d.c.name)
                                    .call((text) =>
                                        text
                                            .append('tspan')
                                            .attr('fill-opacity', 0.7)
                                            .attr('font-weight', 'normal')
                                            .attr('x', -6)
                                            .attr('dy', '1.15em')
                                    ),
                            (update) => update,
                            (exit) =>
                                exit
                                    .transition(transition)
                                    .remove()
                                    .attr(
                                        'transform',
                                        (d) =>
                                            `translate(${x(d.c.value)},${y(
                                                d.rank
                                            )})`
                                    )
                                    .call((g) =>
                                        g
                                            .select('tspan')
                                            .tween('text', (d) =>
                                                textTween(d.c.value, d.c.value)
                                            )
                                    )
                        )
                        .call((bar) =>
                            bar
                                .transition(transition)
                                .attr(
                                    'transform',
                                    (d) =>
                                        `translate(${x(d.c.value)},${y(
                                            d.c.rank
                                        )})`
                                )
                                .call((g) =>
                                    g
                                        .select('tspan')
                                        .tween('text', (d) =>
                                            textTween(
                                                ((d.p && d.p) || d.c).value,
                                                d.c.value
                                            )
                                        )
                                )
                        ));
            }
            const updateLabels = labels(svg);

            function ticker(theSvg) {
                const now = theSvg
                    .append('text')
                    .style('font-weight', `bold`)
                    .style('font-size', `${barSize}px`)
                    .style('font-variant-numeric', 'tabular-nums')
                    .attr('text-anchor', 'end')
                    .attr('x', width - 6)
                    .attr('y', margin.top + barSize * (n - 0.45))
                    .attr('dy', '0.32em')
                    .text(0);

                return (round, transition) => {
                    transition.end().then(() => now.text(round));
                };
            }
            const updateTicker = ticker(svg);

            graph.current = {
                svg,
                updateBars,
                updateAxis,
                updateLabels,
                updateTicker,
            };
        }
        return () => {
            if (graph.current) {
                console.log('****** Removing SVG ******');
                graph.current.svg.remove();
                graph.current = null;
            }
        };
    }, [totalCapys]);
    const prevData = useRef();
    useEffect(() => {
        if (!graph.current) {
            return;
        }
        const { svg, updateBars, updateAxis, updateLabels, updateTicker } =
            graph.current;
        const transition = svg.transition().duration(250).ease(d3.easeLinear);
        const dataItems = existingCapys.map((c, i) => ({
            name: c.name,
            value: c.score,
            rank: i,
        }));
        const data = dataItems.map((c) => ({
            p: prevData?.current?.find?.((p) => p.name === c.name),
            c,
        }));
        prevData.current = dataItems;
        x.domain([0, +dataItems[0].value + 0.2 * +dataItems[0].value]);

        updateAxis(data, transition);
        updateBars(data, transition);
        updateLabels(data, transition);
        updateTicker(round, transition);
    }, [existingCapys]);
    if (!lottery) {
        return (
            <h5>
                Lottery <b>{id}</b> not found or loading.
            </h5>
        );
    }

    return (
        <>
            <div style={{ alignSelf: 'left', width: '100%' }}>
                Lottery{' '}
                <Link href={`/lotteries/${objectId}`}>
                    <a>
                        <b>
                            #{objectId.substring(0, 5)}…
                            {objectId.substring(objectId.length - 5)}
                        </b>
                    </a>
                </Link>
                {' | '}
                <b>{STATUS_TO_TXT[status]}</b>
                {' | '}
                <b>Round {round}</b>
                {' | '}
                <b>{totalCapys} capys</b>
                {STATUS_ENDED === status ? (
                    <h1 style={{ textAlign: 'center' }}>
                        🥳🎉 Winner {existingCapys[0].name} 🎉🥳
                    </h1>
                ) : null}
            </div>
            <div style={{ alignSelf: 'stretch' }} ref={graphNode} />
        </>
    );
};

export default GraphPage;
