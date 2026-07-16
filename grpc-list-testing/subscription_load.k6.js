// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//
// Long-lived SubscriptionService load model. MODE selects smoke, cohort-probe,
// steady, ramp, or churn. Runtime inputs configure the target, fixture profiles,
// VU/arrival targets, stream multiplexing, durations, and stream deadlines.

import { sleep } from 'k6';
import { SharedArray } from 'k6/data';
import exec from 'k6/execution';
import { Counter, Rate, Trend } from 'k6/metrics';
import grpc from 'k6/net/grpc';

const HOST = __ENV.HOST || 'localhost:9000';
const PLAINTEXT = __ENV.PLAINTEXT === '1';
const PROTO_ROOT = __ENV.PROTO_ROOT || '/proto';
const PROTO_FILE = __ENV.PROTO_FILE || 'sui/rpc/v2/subscription_service.proto';
const SUBSCRIPTION_FILE = __ENV.SUBSCRIPTION_FILE || './correctness/subscription_cases.testnet.jsonl';
const MODE = typeof __ENV.MODE === 'undefined' ? 'smoke' : __ENV.MODE;
const PROFILE_IDS_VALUE = typeof __ENV.PROFILE_IDS === 'undefined' ? 'cp.unfiltered' : __ENV.PROFILE_IDS;

const METHODS = {
  SubscribeCheckpoints: 'sui.rpc.v2.SubscriptionService/SubscribeCheckpoints',
  SubscribeTransactions: 'sui.rpc.v2.SubscriptionService/SubscribeTransactions',
  SubscribeEvents: 'sui.rpc.v2.SubscriptionService/SubscribeEvents',
};

const STREAMS_PER_CONNECTION = positiveInteger('STREAMS_PER_CONNECTION', 1);
const MAX_RECV_MB = nonnegativeNumber('MAX_RECV_MB', 0);
const SMOKE_TIMEOUT_SECONDS = positiveNumber('SMOKE_TIMEOUT_SECONDS', 30);
const COHORT_PROBE_TIMEOUT_SECONDS = positiveNumber('COHORT_PROBE_TIMEOUT_SECONDS', 5);
const CHURN_TIMEOUT_SECONDS = positiveNumber('CHURN_TIMEOUT_SECONDS', 30);
const RECONNECT_DELAY_SECONDS = positiveNumber('RECONNECT_DELAY_SECONDS', 1);

const VUS = positiveInteger('VUS', 1);
const START_VUS = positiveInteger('START_VUS', 1);
const MAX_VUS = positiveInteger('MAX_VUS', MODE === 'churn' ? 100 : 1);
const STEP_VUS = positiveInteger('STEP_VUS', 1);
const PRE_ALLOCATED_VUS = positiveInteger('PRE_ALLOCATED_VUS', 10);

const START_SESSIONS_PER_SEC = positiveNumber('START_SESSIONS_PER_SEC', 1);
const MAX_SESSIONS_PER_SEC = positiveNumber('MAX_SESSIONS_PER_SEC', 1);
const STEP_SESSIONS_PER_SEC = positiveNumber('STEP_SESSIONS_PER_SEC', 1);

const DUR = positiveDuration('DUR', '60s');
const RAMP_DUR = positiveDuration('RAMP_DUR', '10s');
const STEP_DUR = positiveDuration('STEP_DUR', '60s');

validateConfiguration();

const requestedProfileIds = parseRequestedProfileIds(PROFILE_IDS_VALUE);
const selectedProfiles = new SharedArray('subscription profiles', function () {
  const rows = [];
  const fixtureIds = {};
  const lines = open(SUBSCRIPTION_FILE).split('\n');

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index].trim();
    if (line.length === 0) continue;

    let row;
    try {
      row = JSON.parse(line);
    } catch (error) {
      throw new Error(`invalid subscription fixture JSON on line ${index + 1}`);
    }

    validateFixtureRow(row, index + 1);
    if (hasOwn(fixtureIds, row.id)) {
      throw new Error(`duplicate subscription fixture id "${row.id}"`);
    }
    fixtureIds[row.id] = true;
    rows.push(row);
  }

  for (let index = 0; index < requestedProfileIds.length; index += 1) {
    const profileId = requestedProfileIds[index];
    if (!hasOwn(fixtureIds, profileId)) {
      throw new Error(`unknown PROFILE_IDS entry "${profileId}"`);
    }
  }

  const selected = rows.filter(function (row) {
    return requestedProfileIds.indexOf(row.id) !== -1;
  });
  if (selected.length === 0) throw new Error('PROFILE_IDS must select at least one profile');
  return selected;
});

validateModeProfileSelection();
if (__ENV.SELF_TEST === '1') runSelfTest();

const client = new grpc.Client();
client.load([PROTO_ROOT], PROTO_FILE);
let connected = false;
let errorsLogged = 0;

const subscriptionConnectAttempts = new Counter('subscription_connect_attempts');
const subscriptionConnectErrors = new Counter('subscription_connect_errors');
const subscriptionStreamsStarted = new Counter('subscription_streams_started');
const subscriptionFirstFrames = new Counter('subscription_first_frames');
const subscriptionFrames = new Counter('subscription_frames');
const subscriptionPayloadFrames = new Counter('subscription_payload_frames');
const subscriptionProgressFrames = new Counter('subscription_progress_frames');
const subscriptionInvalidFrames = new Counter('subscription_invalid_frames');
const subscriptionCursorRegressions = new Counter('subscription_cursor_regressions');
const subscriptionUnexpectedEnds = new Counter('subscription_unexpected_ends');
const subscriptionStreamErrors = new Counter('subscription_stream_errors');
const subscriptionStreamErrorsByKind = new Counter('subscription_stream_errors_by_kind');
const subscriptionCohortRestarts = new Counter('subscription_cohort_restarts');
const subscriptionChurnSessionsOk = new Counter('subscription_churn_sessions_ok');
const subscriptionChurnSessionsFailed = new Counter('subscription_churn_sessions_failed');
const subscriptionTtffMs = new Trend('subscription_ttff_ms', true);
const subscriptionFrameGapMs = new Trend('subscription_frame_gap_ms', true);
const subscriptionSessionMs = new Trend('subscription_session_ms', true);
const subscriptionSmokeOk = new Rate('subscription_smoke_ok');
const subscriptionCohortProbeOk = new Rate('subscription_cohort_probe_ok');

export const options = buildOptions();

export function setup() {
  console.log(buildConfigLine());
}

export async function smoke() {
  const profile = selectedProfiles[0];
  const connectResult = connectClient(profile);
  if (!connectResult.ok) {
    subscriptionSmokeOk.add(false, metricTags(profile));
    return;
  }

  let handle;
  try {
    handle = openSubscription(profile, SMOKE_TIMEOUT_SECONDS);
  } catch (error) {
    recordOpenFailure(profile, error);
    closeClient();
    subscriptionSmokeOk.add(false, metricTags(profile));
    return;
  }

  const firstFrame = await handle.firstFrame;
  if (!firstFrame.ok) {
    handle.markExpectedClose();
    closeClient();
    subscriptionSmokeOk.add(false, metricTags(profile));
    return;
  }

  handle.recordSession();
  handle.markExpectedClose();
  closeClient();
  subscriptionSmokeOk.add(true, metricTags(profile));
  console.log(`SUBSCRIPTION-SMOKE-OK profile=${profile.id}`);
}

export async function cohortProbe() {
  let handles = [];
  let resultRecorded = false;
  const accounting = {
    streamsStarted: 0,
    validFirstFrames: 0,
    deadlineTerminals: 0,
    restarts: 0,
  };

  function finish(ok) {
    if (resultRecorded) return;
    resultRecorded = true;
    subscriptionCohortProbeOk.add(ok, { mode: MODE });
    if (ok) console.log('SUBSCRIPTION-COHORT-PROBE-OK');
  }

  try {
    let connectResult = connectClient(selectedProfiles[0]);
    if (!connectResult.ok) {
      finish(false);
      return;
    }

    handles = openProbeCohort(true);
    if (handles === null) {
      finish(false);
      return;
    }
    accounting.streamsStarted += handles.length;

    const firstCohortReadiness = await awaitCohortReadiness(handles);
    if (firstCohortReadiness.type !== 'ready') {
      closeExpected(handles);
      finish(false);
      return;
    }
    accounting.validFirstFrames += countValidFirstFrames(firstCohortReadiness.results);
    if (accounting.validFirstFrames !== handles.length) {
      closeExpected(handles);
      finish(false);
      return;
    }

    const terminal = await Promise.race(handles.map(function (handle) {
      return handle.terminal;
    }));
    if (terminal.expected || terminal.type !== 'error' || terminal.kind !== 'deadline' || terminal.profileId !== handles[0].profile.id) {
      closeExpected(handles);
      finish(false);
      return;
    }

    accounting.deadlineTerminals += 1;
    subscriptionCohortRestarts.add(1, metricTags(handles[0].profile));
    accounting.restarts += 1;
    closeExpected(handles);
    sleep(RECONNECT_DELAY_SECONDS);

    connectResult = connectClient(selectedProfiles[0]);
    if (!connectResult.ok) {
      finish(false);
      return;
    }

    handles = openProbeCohort(false);
    if (handles === null) {
      finish(false);
      return;
    }
    accounting.streamsStarted += handles.length;

    const secondCohortReadiness = await awaitCohortReadiness(handles);
    if (secondCohortReadiness.type !== 'ready') {
      closeExpected(handles);
      finish(false);
      return;
    }
    accounting.validFirstFrames += countValidFirstFrames(secondCohortReadiness.results);
    if (accounting.streamsStarted !== 4
        || accounting.validFirstFrames !== 4
        || accounting.deadlineTerminals !== 1
        || accounting.restarts !== 1) {
      closeExpected(handles);
      finish(false);
      return;
    }

    closeExpected(handles);
    finish(true);
  } catch (error) {
    closeExpected(handles);
    logBoundedError('cohort-probe-err', selectedProfiles[0], error, errKind(error));
    finish(false);
  }
}

export async function holdSubscriptions() {
  const profiles = [];
  for (let offset = 0; offset < STREAMS_PER_CONNECTION; offset += 1) {
    profiles.push(selectedProfiles[(__VU - 1 + offset) % selectedProfiles.length]);
  }

  const connectResult = connectClient(profiles[0]);
  if (!connectResult.ok) {
    sleep(RECONNECT_DELAY_SECONDS);
    return;
  }

  const handles = [];
  for (let offset = 0; offset < profiles.length; offset += 1) {
    try {
      handles.push(openSubscription(profiles[offset]));
    } catch (error) {
      markExpected(handles);
      recordOpenFailure(profiles[offset], error);
      closeClient();
      sleep(RECONNECT_DELAY_SECONDS);
      return;
    }
  }

  const terminal = await Promise.race(handles.map(function (handle) {
    return handle.terminal;
  }));
  subscriptionCohortRestarts.add(1, metricTags(terminal.profile));
  markExpected(handles);
  closeClient();
  sleep(RECONNECT_DELAY_SECONDS);
}

export async function churn() {
  const profile = selectedProfiles[exec.scenario.iterationInTest % selectedProfiles.length];
  const connectResult = connectClient(profile);
  if (!connectResult.ok) {
    subscriptionChurnSessionsFailed.add(1, metricTags(profile));
    return;
  }

  let handle;
  try {
    handle = openSubscription(profile, CHURN_TIMEOUT_SECONDS);
  } catch (error) {
    recordOpenFailure(profile, error);
    closeClient();
    subscriptionChurnSessionsFailed.add(1, metricTags(profile));
    return;
  }

  const firstFrame = await handle.firstFrame;
  if (!firstFrame.ok) {
    handle.markExpectedClose();
    closeClient();
    subscriptionChurnSessionsFailed.add(1, metricTags(profile));
    return;
  }

  handle.recordSession();
  handle.markExpectedClose();
  closeClient();
  subscriptionChurnSessionsOk.add(1, metricTags(profile));
}

function buildOptions() {
  const thresholds = {};
  let scenario;

  if (MODE === 'smoke') {
    thresholds.subscription_smoke_ok = ['rate==1'];
    scenario = {
      executor: 'per-vu-iterations',
      exec: 'smoke',
      vus: 1,
      iterations: 1,
      maxDuration: `${SMOKE_TIMEOUT_SECONDS + 5}s`,
    };
  } else if (MODE === 'cohort-probe') {
    thresholds.subscription_cohort_probe_ok = ['rate==1'];
    scenario = {
      executor: 'per-vu-iterations',
      exec: 'cohortProbe',
      vus: 1,
      iterations: 1,
      maxDuration: `${COHORT_PROBE_TIMEOUT_SECONDS + SMOKE_TIMEOUT_SECONDS + RECONNECT_DELAY_SECONDS + 10}s`,
    };
  } else if (MODE === 'steady') {
    scenario = {
      executor: 'constant-vus',
      exec: 'holdSubscriptions',
      vus: VUS,
      duration: DUR,
      gracefulStop: '0s',
    };
  } else if (MODE === 'ramp') {
    scenario = {
      executor: 'ramping-vus',
      exec: 'holdSubscriptions',
      startVUs: 0,
      stages: buildRampStages(),
      gracefulRampDown: '0s',
    };
  } else {
    thresholds.dropped_iterations = ['count==0'];
    scenario = {
      executor: 'ramping-arrival-rate',
      exec: 'churn',
      startRate: START_SESSIONS_PER_SEC,
      timeUnit: '1s',
      preAllocatedVUs: PRE_ALLOCATED_VUS,
      maxVUs: MAX_VUS,
      stages: buildChurnStages(),
      gracefulStop: `${CHURN_TIMEOUT_SECONDS + 5}s`,
    };
  }

  const scenarios = {};
  scenarios[MODE] = scenario;
  return {
    scenarios,
    thresholds,
    insecureSkipTLSVerify: true,
  };
}

function buildRampStages() {
  const stages = [];
  const targets = buildTargets(START_VUS, MAX_VUS, STEP_VUS);
  for (let index = 0; index < targets.length; index += 1) {
    stages.push({ target: targets[index], duration: RAMP_DUR });
    stages.push({ target: targets[index], duration: STEP_DUR });
  }
  stages.push({ target: 0, duration: RAMP_DUR });
  return stages;
}

function buildChurnStages() {
  return buildTargets(START_SESSIONS_PER_SEC, MAX_SESSIONS_PER_SEC, STEP_SESSIONS_PER_SEC).map(function (target) {
    return { target, duration: STEP_DUR };
  });
}

function buildTargets(start, maximum, step) {
  const targets = [];
  for (let target = start; target <= maximum; target += step) targets.push(target);
  if (targets[targets.length - 1] !== maximum) targets.push(maximum);
  return targets;
}

function buildConfigLine() {
  let connectionTarget;
  let intendedPeakStreams;
  let timing;

  if (MODE === 'smoke') {
    connectionTarget = 'vus=1';
    intendedPeakStreams = 1;
    timing = `timeout=${SMOKE_TIMEOUT_SECONDS}s max_duration=${SMOKE_TIMEOUT_SECONDS + 5}s`;
  } else if (MODE === 'cohort-probe') {
    connectionTarget = 'vus=1';
    intendedPeakStreams = 2;
    timing = `probe_timeout=${COHORT_PROBE_TIMEOUT_SECONDS}s smoke_timeout=${SMOKE_TIMEOUT_SECONDS}s reconnect_delay=${RECONNECT_DELAY_SECONDS}s`;
  } else if (MODE === 'steady') {
    connectionTarget = `vus=${VUS}`;
    intendedPeakStreams = VUS * STREAMS_PER_CONNECTION;
    timing = `duration=${DUR} reconnect_delay=${RECONNECT_DELAY_SECONDS}s`;
  } else if (MODE === 'ramp') {
    connectionTarget = `start_vus=${START_VUS} max_vus=${MAX_VUS} step_vus=${STEP_VUS}`;
    intendedPeakStreams = MAX_VUS * STREAMS_PER_CONNECTION;
    timing = `ramp_duration=${RAMP_DUR} step_duration=${STEP_DUR} reconnect_delay=${RECONNECT_DELAY_SECONDS}s`;
  } else {
    connectionTarget = `preallocated_vus=${PRE_ALLOCATED_VUS} max_vus=${MAX_VUS}`;
    intendedPeakStreams = MAX_VUS;
    timing = `start_rate=${START_SESSIONS_PER_SEC} max_rate=${MAX_SESSIONS_PER_SEC} step_rate=${STEP_SESSIONS_PER_SEC} step_duration=${STEP_DUR} timeout=${CHURN_TIMEOUT_SECONDS}s`;
  }

  return `SUBSCRIPTION-CONFIG mode=${MODE} host=${HOST.slice(0, 120)} profiles=${selectedProfiles.map(function (profile) { return profile.id; }).join(',')} ${connectionTarget} streams_per_connection=${STREAMS_PER_CONNECTION} intended_peak_logical_streams=${intendedPeakStreams} ${timing}`;
}

function connectClient(profile) {
  const tags = metricTags(profile);
  subscriptionConnectAttempts.add(1, tags);
  const connectParams = { plaintext: PLAINTEXT, timeout: '10s' };
  if (MAX_RECV_MB > 0) connectParams.maxReceiveSize = MAX_RECV_MB * 1024 * 1024;

  try {
    client.connect(HOST, connectParams);
    connected = true;
    return { ok: true };
  } catch (error) {
    const kind = errKind(error);
    subscriptionConnectErrors.add(1, errorMetricTags(profile, kind));
    logBoundedError('connect-err', profile, error, kind);
    closeClient();
    return { ok: false, kind };
  }
}

function closeClient() {
  client.close();
  connected = false;
}

function openSubscription(profile, timeoutSeconds = 0, recordUnexpectedSession = MODE === 'steady' || MODE === 'ramp') {
  const tags = metricTags(profile);
  const startedAt = Date.now();
  const state = newFrameState();
  let expectedClose = false;
  let settled = false;
  let firstFrameSettled = false;
  let sessionRecorded = false;
  let resolveFirstFrame;
  let resolveTerminal;

  const firstFrame = new Promise(function (resolve) {
    resolveFirstFrame = resolve;
  });
  const terminal = new Promise(function (resolve) {
    resolveTerminal = resolve;
  });

  const streamOptions = timeoutSeconds > 0 ? { timeout: `${timeoutSeconds}s` } : undefined;
  const stream = streamOptions
    ? new grpc.Stream(client, METHODS[profile.rpc], streamOptions)
    : new grpc.Stream(client, METHODS[profile.rpc]);

  subscriptionStreamsStarted.add(1, tags);

  function settleFirstFrame(result) {
    if (firstFrameSettled) return;
    firstFrameSettled = true;
    resolveFirstFrame(result);
  }

  function recordSession() {
    if (sessionRecorded) return;
    sessionRecorded = true;
    subscriptionSessionMs.add(Date.now() - startedAt, tags);
  }

  function finishTerminal(type, error) {
    if (settled) return;
    settled = true;

    if (expectedClose) {
      settleFirstFrame({ ok: false, expected: true });
      resolveTerminal({ expected: true, type, profile, profileId: profile.id });
      return;
    }

    const kind = type === 'error' ? errKind(error) : 'end';
    if (recordUnexpectedSession) recordSession();
    if (type === 'error') {
      subscriptionStreamErrors.add(1, tags);
      subscriptionStreamErrorsByKind.add(1, errorMetricTags(profile, kind));
      logBoundedError('stream-err', profile, error, kind);
    } else {
      subscriptionUnexpectedEnds.add(1, tags);
      logBoundedError('stream-end', profile, null, kind);
    }

    settleFirstFrame({ ok: false, expected: false, kind, type });
    resolveTerminal({ expected: false, kind, type, profile, profileId: profile.id });
  }

  stream.on('data', function (message) {
    subscriptionFrames.add(1, tags);
    const firstResponse = state.responsesSeen === 0;
    const observation = observeFrame(profile, message, state);
    if (!firstResponse || firstFrameSettled) return;

    if (!observation.valid) {
      settleFirstFrame({ ok: false, expected: false, kind: 'invalid', type: 'frame' });
      return;
    }

    subscriptionFirstFrames.add(1, tags);
    subscriptionTtffMs.add(Date.now() - startedAt, tags);
    settleFirstFrame({ ok: true });
  });
  stream.on('error', function (error) {
    finishTerminal('error', error);
  });
  stream.on('end', function () {
    finishTerminal('end', null);
  });

  try {
    stream.write(profile.request);
    stream.end();
  } catch (error) {
    expectedClose = true;
    throw error;
  }

  return {
    firstFrame,
    terminal,
    profile,
    markExpectedClose: function () {
      expectedClose = true;
    },
    recordSession,
  };
}

function openProbeCohort(firstCohort) {
  const handles = [];
  for (let offset = 0; offset < selectedProfiles.length; offset += 1) {
    const timeout = firstCohort && offset === 0 ? COHORT_PROBE_TIMEOUT_SECONDS : firstCohort ? 0 : SMOKE_TIMEOUT_SECONDS;
    try {
      handles.push(openSubscription(selectedProfiles[offset], timeout, firstCohort));
    } catch (error) {
      markExpected(handles);
      recordOpenFailure(selectedProfiles[offset], error);
      closeClient();
      return null;
    }
  }
  return handles;
}

async function awaitCohortReadiness(handles) {
  const readiness = Promise.all(handles.map(function (handle) {
    return handle.firstFrame;
  })).then(function (results) {
    return { type: 'ready', results };
  });
  const firstTerminal = Promise.race(handles.map(function (handle) {
    return handle.terminal;
  })).then(function (terminal) {
    return { type: 'terminal', terminal };
  });
  return Promise.race([readiness, firstTerminal]);
}

function countValidFirstFrames(results) {
  let valid = 0;
  for (let index = 0; index < results.length; index += 1) {
    if (results[index].ok) valid += 1;
  }
  return valid;
}

function closeExpected(handles) {
  markExpected(handles);
  if (connected) closeClient();
}

function markExpected(handles) {
  for (let index = 0; index < handles.length; index += 1) {
    handles[index].markExpectedClose();
  }
}

function recordOpenFailure(profile, error) {
  subscriptionStreamErrors.add(1, metricTags(profile));
  subscriptionStreamErrorsByKind.add(1, errorMetricTags(profile, 'open'));
  logBoundedError('stream-open-err', profile, error, 'open');
}

function newFrameState() {
  return {
    responsesSeen: 0,
    lastCursor: null,
    watermarkCheckpointSeen: false,
    lastWatermarkCheckpoint: null,
    lastPayloadPosition: null,
    lastValidFrameAt: null,
  };
}

function observeFrame(profile, message, state) {
  const tags = metricTags(profile);
  const firstResponse = state.responsesSeen === 0;
  state.responsesSeen += 1;

  try {
    if (!message || typeof message !== 'object' || Array.isArray(message)) {
      failFrame('response is not an object', false);
    }

    let payload;
    if (profile.rpc === 'SubscribeCheckpoints') {
      payload = validateCheckpointFrame(profile, message, state, firstResponse);
    } else if (profile.rpc === 'SubscribeTransactions') {
      payload = validateWatermarkedFrame(profile, message, state, firstResponse, 'transaction');
    } else {
      payload = validateWatermarkedFrame(profile, message, state, firstResponse, 'event');
    }

    const now = Date.now();
    if (state.lastValidFrameAt !== null) {
      subscriptionFrameGapMs.add(now - state.lastValidFrameAt, tags);
    }
    state.lastValidFrameAt = now;
    if (payload) subscriptionPayloadFrames.add(1, tags);
    else subscriptionProgressFrames.add(1, tags);
    return { valid: true, payload };
  } catch (error) {
    subscriptionInvalidFrames.add(1, tags);
    if (error && error.cursorRegression) subscriptionCursorRegressions.add(1, tags);
    logBoundedError('invalid-frame', profile, error, 'invalid');
    return { valid: false, payload: false };
  }
}

function validateCheckpointFrame(profile, message, state, firstResponse) {
  if (!hasOwn(message, 'cursor')) failFrame('checkpoint cursor is absent', false);
  const cursor = normalizeUint64(message.cursor, 'checkpoint cursor');
  if (state.lastCursor !== null) {
    const cursorOrder = compareUnsignedDecimal(cursor, state.lastCursor);
    if (cursorOrder <= 0) failFrame('checkpoint cursor did not advance', true);
    if (!hasOwn(profile.request, 'filter') && cursor !== incrementUnsignedDecimal(state.lastCursor)) {
      failFrame('unfiltered checkpoint cursor skipped a sequence number', false);
    }
  }

  const hasPayload = hasOwn(message, 'checkpoint') && message.checkpoint !== null && typeof message.checkpoint !== 'undefined';
  const filtered = hasOwn(profile.request, 'filter');
  if (firstResponse && filtered && hasPayload) failFrame('filtered first response contains a checkpoint', false);
  if (!filtered && !hasPayload) failFrame('unfiltered checkpoint response is progress-only', false);

  if (hasPayload) {
    const checkpoint = message.checkpoint;
    requireObject(checkpoint, 'checkpoint payload');
    requireRequestedFields(checkpoint, requestedFields(profile), 'checkpoint payload');
    if (!hasOwn(checkpoint, 'sequenceNumber')) failFrame('checkpoint sequenceNumber is absent', false);
    if (!hasOwn(checkpoint, 'digest')) failFrame('checkpoint digest is absent', false);
    const sequenceNumber = normalizeUint64(checkpoint.sequenceNumber, 'checkpoint sequenceNumber');
    if (sequenceNumber !== cursor) failFrame('checkpoint sequenceNumber differs from cursor', false);
  }

  state.lastCursor = cursor;
  return hasPayload;
}

function validateWatermarkedFrame(profile, message, state, firstResponse, payloadField) {
  if (!hasOwn(message, 'watermark')) failFrame('watermark is absent', false);
  const watermark = message.watermark;
  requireObject(watermark, 'watermark');
  if (!hasOwn(watermark, 'cursor') || !isNonemptyOpaqueCursor(watermark.cursor)) {
    failFrame('watermark cursor is empty', false);
  }

  const checkpointPresent = hasOwn(watermark, 'checkpoint') && watermark.checkpoint !== null && typeof watermark.checkpoint !== 'undefined';
  let watermarkCheckpoint = null;
  if (checkpointPresent) {
    watermarkCheckpoint = normalizeUint64(watermark.checkpoint, 'watermark checkpoint');
    if (state.watermarkCheckpointSeen && compareUnsignedDecimal(watermarkCheckpoint, state.lastWatermarkCheckpoint) < 0) {
      failFrame('watermark checkpoint regressed', true);
    }
  } else if (state.watermarkCheckpointSeen) {
    failFrame('watermark checkpoint became absent', true);
  }

  const hasPayload = hasOwn(message, payloadField) && message[payloadField] !== null && typeof message[payloadField] !== 'undefined';
  if (firstResponse && hasOwn(profile.request, 'filter') && hasPayload) {
    failFrame('filtered first response contains a payload', false);
  }

  if (hasPayload) {
    const payload = message[payloadField];
    requireObject(payload, `${payloadField} payload`);
    requireRequestedFields(payload, requestedFields(profile), `${payloadField} payload`);
    const position = payloadField === 'transaction'
      ? transactionPosition(payload)
      : eventPosition(payload);
    if (state.lastPayloadPosition !== null && comparePosition(position, state.lastPayloadPosition) <= 0) {
      failFrame(`${payloadField} payload position regressed`, true);
    }
    state.lastPayloadPosition = position;
  }

  if (checkpointPresent) {
    state.watermarkCheckpointSeen = true;
    state.lastWatermarkCheckpoint = watermarkCheckpoint;
  }
  return hasPayload;
}

function transactionPosition(transaction) {
  if (!hasOwn(transaction, 'digest')) failFrame('transaction digest is absent', false);
  if (!hasOwn(transaction, 'checkpoint')) failFrame('transaction checkpoint is absent', false);
  if (!hasOwn(transaction, 'transactionIndex')) failFrame('transaction transactionIndex is absent', false);
  return [
    normalizeUint64(transaction.checkpoint, 'transaction checkpoint'),
    normalizeUint64(transaction.transactionIndex, 'transaction transactionIndex'),
  ];
}

function eventPosition(event) {
  if (!hasOwn(event, 'transactionDigest')) failFrame('event transactionDigest is absent', false);
  if (!hasOwn(event, 'checkpoint')) failFrame('event checkpoint is absent', false);
  if (!hasOwn(event, 'transactionIndex')) failFrame('event transactionIndex is absent', false);
  if (!hasOwn(event, 'eventIndex')) failFrame('event eventIndex is absent', false);
  return [
    normalizeUint64(event.checkpoint, 'event checkpoint'),
    normalizeUint64(event.transactionIndex, 'event transactionIndex'),
    normalizeUint32(event.eventIndex, 'event eventIndex'),
  ];
}

function requestedFields(profile) {
  return profile.request.read_mask.split(',').map(function (field) {
    return field.trim();
  }).filter(function (field) {
    return field.length > 0;
  });
}

function requireRequestedFields(payload, fields, payloadName) {
  for (let index = 0; index < fields.length; index += 1) {
    if (!hasOwn(payload, fields[index])) failFrame(`${payloadName} field ${fields[index]} is absent`, false);
  }
}

function requireObject(value, fieldName) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    failFrame(`${fieldName} is not an object`, false);
  }
}

function isNonemptyOpaqueCursor(value) {
  if (typeof value === 'string') return value.length > 0;
  if (Array.isArray(value)) return value.length > 0;
  return false;
}

function comparePosition(left, right) {
  for (let index = 0; index < left.length; index += 1) {
    const order = compareUnsignedDecimal(left[index], right[index]);
    if (order !== 0) return order;
  }
  return 0;
}

function failFrame(message, cursorRegression) {
  const error = new Error(message);
  error.cursorRegression = cursorRegression;
  throw error;
}

function normalizeUint64(value, fieldName) {
  if (typeof value !== 'string' || !isCanonicalUnsignedDecimal(value)) {
    throw new Error(`${fieldName} is not a canonical uint64 string`);
  }
  if (compareUnsignedDecimal(value, '18446744073709551615') > 0) {
    throw new Error(`${fieldName} exceeds uint64`);
  }
  return value;
}

function normalizeUint32(value, fieldName) {
  let normalized;
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value) || value < 0) {
      throw new Error(`${fieldName} is not a nonnegative safe integer`);
    }
    normalized = String(value);
  } else if (typeof value === 'string' && isCanonicalUnsignedDecimal(value)) {
    normalized = value;
  } else {
    throw new Error(`${fieldName} is not a canonical uint32 value`);
  }

  if (compareUnsignedDecimal(normalized, '4294967295') > 0) {
    throw new Error(`${fieldName} exceeds uint32`);
  }
  return normalized;
}

function compareUnsignedDecimal(left, right) {
  if (!isCanonicalUnsignedDecimal(left) || !isCanonicalUnsignedDecimal(right)) {
    throw new Error('unsigned decimal comparison requires canonical strings');
  }
  if (left.length < right.length) return -1;
  if (left.length > right.length) return 1;
  if (left < right) return -1;
  if (left > right) return 1;
  return 0;
}

function incrementUnsignedDecimal(value) {
  if (!isCanonicalUnsignedDecimal(value)) {
    throw new Error('unsigned decimal increment requires a canonical string');
  }

  const digits = value.split('');
  let carry = 1;
  for (let index = digits.length - 1; index >= 0 && carry === 1; index -= 1) {
    const digit = digits[index].charCodeAt(0) - 48 + carry;
    digits[index] = String(digit % 10);
    carry = digit >= 10 ? 1 : 0;
  }
  if (carry === 1) digits.unshift('1');
  return digits.join('');
}

function isCanonicalUnsignedDecimal(value) {
  return typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value);
}

function runSelfTest() {
  const absent = {};
  if (normalizeUint64('0', 'self-test uint64') !== '0') throw new Error('SELF_TEST failed: uint64 zero');
  if (normalizeUint32(0, 'self-test uint32') !== '0') throw new Error('SELF_TEST failed: uint32 zero');
  if (hasOwn(absent, 'optional')) throw new Error('SELF_TEST failed: absent optional property');
  if (incrementUnsignedDecimal('999') !== '1000') throw new Error('SELF_TEST failed: decimal increment');
  if (compareUnsignedDecimal('9007199254740993', '9007199254740992') <= 0) {
    throw new Error('SELF_TEST failed: lossless decimal ordering');
  }
  if (errKind({ code: 14 }) !== 'unavailable') throw new Error('SELF_TEST failed: numeric unavailable status');
  if (errKind({ code: 'Unavailable' }) !== 'unavailable') throw new Error('SELF_TEST failed: string unavailable status');
  if (errKind({ code: { toString: function () { return 'Unavailable'; } } }) !== 'unavailable') {
    throw new Error('SELF_TEST failed: wrapped unavailable status');
  }
  if (errKind({ code: 'DeadlineExceeded' }) !== 'deadline') throw new Error('SELF_TEST failed: string deadline status');
}

function validateConfiguration() {
  const validModes = ['smoke', 'cohort-probe', 'steady', 'ramp', 'churn'];
  if (validModes.indexOf(MODE) === -1) {
    throw new Error(`invalid MODE "${MODE}"; expected smoke, cohort-probe, steady, ramp, or churn`);
  }
  if ((MODE === 'smoke' || MODE === 'churn') && STREAMS_PER_CONNECTION !== 1) {
    throw new Error('STREAMS_PER_CONNECTION must be 1 when MODE is smoke or churn');
  }
  if (START_VUS > MAX_VUS) throw new Error('START_VUS must be less than or equal to MAX_VUS');
  if (START_SESSIONS_PER_SEC > MAX_SESSIONS_PER_SEC) {
    throw new Error('START_SESSIONS_PER_SEC must be less than or equal to MAX_SESSIONS_PER_SEC');
  }
  if (MODE === 'churn' && PRE_ALLOCATED_VUS > MAX_VUS) throw new Error('PRE_ALLOCATED_VUS must be less than or equal to MAX_VUS');
}

function validateModeProfileSelection() {
  if (MODE === 'smoke' && selectedProfiles.length !== 1) {
    throw new Error('PROFILE_IDS must select exactly one profile when MODE is smoke');
  }
  if (MODE === 'cohort-probe' && (STREAMS_PER_CONNECTION !== 2 || selectedProfiles.length !== 2)) {
    throw new Error('cohort-probe requires STREAMS_PER_CONNECTION=2 and exactly two profiles');
  }
}

function validateFixtureRow(row, lineNumber) {
  if (!row || typeof row !== 'object' || Array.isArray(row)) {
    throw new Error(`invalid subscription fixture row on line ${lineNumber}`);
  }
  const keys = Object.keys(row).sort().join(',');
  if (keys !== 'id,request,rpc' || typeof row.id !== 'string' || row.id.length === 0 || typeof row.rpc !== 'string' || !row.request || typeof row.request !== 'object' || Array.isArray(row.request)) {
    throw new Error(`invalid subscription fixture row on line ${lineNumber}`);
  }
  if (!hasOwn(METHODS, row.rpc)) {
    throw new Error(`unsupported subscription RPC "${row.rpc}" on line ${lineNumber}`);
  }
  if (typeof row.request.read_mask !== 'string' || row.request.read_mask.length === 0) {
    throw new Error(`invalid subscription read_mask on line ${lineNumber}`);
  }
}

function parseRequestedProfileIds(value) {
  if (value.length === 0) throw new Error('PROFILE_IDS must select at least one profile');
  const ids = value.split(',');
  const seen = {};
  for (let index = 0; index < ids.length; index += 1) {
    const id = ids[index];
    if (id.length === 0) throw new Error('PROFILE_IDS must select at least one profile');
    if (hasOwn(seen, id)) throw new Error(`repeated PROFILE_IDS entry "${id}"`);
    seen[id] = true;
  }
  return ids;
}

function positiveInteger(name, defaultValue) {
  const value = envNumber(name, defaultValue);
  if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`${name} must be a positive integer`);
  return value;
}

function positiveNumber(name, defaultValue) {
  const value = envNumber(name, defaultValue);
  if (!Number.isFinite(value) || value <= 0) throw new Error(`${name} must be positive`);
  return value;
}

function nonnegativeNumber(name, defaultValue) {
  const value = envNumber(name, defaultValue);
  if (!Number.isFinite(value) || value < 0) throw new Error(`${name} must be nonnegative`);
  return value;
}

function envNumber(name, defaultValue) {
  if (typeof __ENV[name] === 'undefined') return defaultValue;
  if (__ENV[name].length === 0) throw new Error(`${name} must be numeric`);
  return Number(__ENV[name]);
}

function positiveDuration(name, defaultValue) {
  const value = typeof __ENV[name] === 'undefined' ? defaultValue : __ENV[name];
  const matches = value.match(/([0-9]+(?:\.[0-9]+)?)(ms|s|m|h)/g);
  if (!matches || matches.join('') !== value) throw new Error(`${name} must be a positive k6 duration`);

  const unitMilliseconds = { ms: 1, s: 1000, m: 60000, h: 3600000 };
  let milliseconds = 0;
  for (let index = 0; index < matches.length; index += 1) {
    const match = /^([0-9]+(?:\.[0-9]+)?)(ms|s|m|h)$/.exec(matches[index]);
    milliseconds += Number(match[1]) * unitMilliseconds[match[2]];
  }
  if (!Number.isFinite(milliseconds) || milliseconds <= 0) throw new Error(`${name} must be a positive k6 duration`);
  return value;
}

function metricTags(profile) {
  return { mode: MODE, profile: profile.id, rpc: profile.rpc };
}

function errorMetricTags(profile, kind) {
  return { mode: MODE, profile: profile.id, rpc: profile.rpc, kind };
}

function errKind(error) {
  const code = error && typeof error.code !== 'undefined' ? error.code : null;
  const codeName = code === null ? '' : String(code).replace(/_/g, '').toLowerCase();
  if (code === 4 || codeName === 'deadlineexceeded') return 'deadline';
  if (code === 1 || codeName === 'canceled' || codeName === 'cancelled') return 'cancelled';
  if (code === 8 || codeName === 'resourceexhausted') return 'resource_exhausted';
  if (code === 14 || codeName === 'unavailable') return 'unavailable';
  if (code === 13 || codeName === 'internal') return 'internal';
  const message = (error && error.message ? String(error.message) : '').toLowerCase();
  if (message.includes('deadline')) return 'deadline';
  if (message.includes('reset') || message.includes('eof') || message.includes('goaway') || message.includes('connection')) return 'conn_reset';
  return 'other';
}

function logBoundedError(prefix, profile, error, kind) {
  if (errorsLogged >= 5) return;
  errorsLogged += 1;
  const code = error && typeof error.code !== 'undefined' ? error.code : '';
  const message = error && error.message ? String(error.message).slice(0, 120) : '';
  console.log(`${prefix} profile=${profile.id} rpc=${profile.rpc} kind=${kind} code=${code} msg=${message}`);
}

function hasOwn(value, field) {
  return Object.prototype.hasOwnProperty.call(value, field);
}
