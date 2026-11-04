// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Hand-rolled HTTP/2 server that speaks just enough gRPC for `PingAndWarm` and `ReadRows`, for
//! tests that need frame-level control no real server allows: it keeps sending DATA frames on
//! streams the client has already reset, the way a BigTable frontend does when the reset is still
//! in flight, and it can send GOAWAY on chosen connections. h2's own server stops writing to a
//! stream once the peer resets it.
//!
//! A request whose gRPC message is empty is answered as `PingAndWarm`; any other request is
//! decoded as `ReadRows` and handled according to the server's [`ReadRowsMode`]. Rows have one
//! cell each, with qualifier `v` and the row key as value.

use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bytes::BufMut;
use bytes::Bytes;
use bytes::BytesMut;
use prost::Message;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tonic::transport::Channel;
use tonic::transport::Endpoint;

use crate::bigtable::proto::bigtable::v2::ReadRowsRequest;
use crate::bigtable::proto::bigtable::v2::ReadRowsResponse;
use crate::bigtable::proto::bigtable::v2::read_rows_response::CellChunk;
use crate::bigtable::proto::bigtable::v2::read_rows_response::cell_chunk::RowStatus;

const CLIENT_PREFACE_LEN: usize = 24;

const FRAME_DATA: u8 = 0x0;
const FRAME_HEADERS: u8 = 0x1;
const FRAME_RST_STREAM: u8 = 0x3;
const FRAME_SETTINGS: u8 = 0x4;
const FRAME_PING: u8 = 0x6;
const FRAME_GOAWAY: u8 = 0x7;
const FRAME_WINDOW_UPDATE: u8 = 0x8;

const FLAG_END_STREAM: u8 = 0x1;
const FLAG_ACK: u8 = 0x1;
const FLAG_END_HEADERS: u8 = 0x4;

const SETTINGS_MAX_CONCURRENT_STREAMS: u16 = 0x3;
const DEFAULT_MAX_CONCURRENT_STREAMS: u32 = 100_000;
/// HTTP/2's default SETTINGS_MAX_FRAME_SIZE, which the client in these tests keeps.
const MAX_FRAME_SIZE: usize = 16_384;
const MAX_WINDOW_INCREMENT: u32 = (1 << 31) - 1 - 65_535;
const NO_ERROR: u32 = 0x0;
/// Commands a connection may have queued; tests issue one at a time.
const COMMAND_QUEUE: usize = 16;
/// Frames read from a connection but not yet processed.
const FRAME_QUEUE: usize = 1024;

type CommandSender = mpsc::Sender<(Command, oneshot::Sender<()>)>;

/// GOAWAY frame received from the client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoAway {
    pub(crate) error_code: u32,
    pub(crate) debug_data: Bytes,
}

/// How the server handles `ReadRows` requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadRowsMode {
    /// Answer with one row per requested key and an OK status.
    Respond,
    /// Send response headers and nothing more until [`RawH2Server::complete_held_streams`].
    HoldAfterHeaders,
    /// Send nothing, not even response headers.
    Ignore,
    /// Answer with HTTP 503 and no gRPC status, as a proxy in front of BigTable can.
    ServiceUnavailable,
}

pub(crate) struct RawH2Server {
    endpoint: Endpoint,
    shared: Arc<Shared>,
    accept_loop: JoinHandle<()>,
}

struct Shared {
    read_rows_mode: Mutex<ReadRowsMode>,
    /// Streams each connection lets the client open at once.
    max_concurrent_streams: u32,
    connections: AtomicUsize,
    /// `ReadRows` requests held or ignored rather than answered, across connections.
    unanswered_requests: AtomicUsize,
    /// Streams the client reset, across connections.
    reset_streams: AtomicUsize,
    commands: Mutex<Vec<CommandSender>>,
    client_goaway: Mutex<Option<GoAway>>,
}

/// Per-connection record of the requests the server has not answered.
#[derive(Default)]
struct Streams {
    /// `ReadRows` streams answered with headers only, with the keys they requested.
    held: HashMap<u32, Vec<Bytes>>,
    /// `ReadRows` streams that got no response at all.
    ignored: HashSet<u32>,
    /// Streams the client reset.
    reset: HashSet<u32>,
}

enum Command {
    /// Send a DATA frame with `payload_len` bytes, without END_STREAM, on every held stream the
    /// client has reset.
    FloodResetStreams { payload_len: usize },
    /// Finish every held stream the client has not reset with its rows and an OK status.
    CompleteHeldStreams,
    /// On every ignored stream the client has reset, send response headers and then a DATA
    /// frame with `payload_len` bytes, without END_STREAM, and forget those streams.
    AnswerIgnoredStreams { payload_len: usize },
    /// On a connection with ignored requests, send a graceful GOAWAY (NO_ERROR) whose last
    /// stream id is 0, telling the client none of its streams were processed, and forget them.
    GoAwayIgnoringConnections,
    /// Close every connection holding a stream the client has not reset, without a GOAWAY.
    CloseHoldingConnections,
}

struct RawFrame {
    kind: u8,
    flags: u8,
    stream_id: u32,
    payload: Bytes,
}

impl RawH2Server {
    pub(crate) async fn start() -> Self {
        Self::start_with_max_concurrent_streams(DEFAULT_MAX_CONCURRENT_STREAMS).await
    }

    /// A server whose connections let the client open at most `max_concurrent_streams` streams
    /// at once; the client queues further requests until a stream closes.
    pub(crate) async fn start_with_max_concurrent_streams(max_concurrent_streams: u32) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shared = Arc::new(Shared {
            read_rows_mode: Mutex::new(ReadRowsMode::Respond),
            max_concurrent_streams,
            connections: AtomicUsize::new(0),
            unanswered_requests: AtomicUsize::new(0),
            reset_streams: AtomicUsize::new(0),
            commands: Mutex::new(Vec::new()),
            client_goaway: Mutex::new(None),
        });
        let accept_loop = tokio::spawn({
            let shared = shared.clone();
            async move {
                while let Ok((stream, _)) = listener.accept().await {
                    shared.connections.fetch_add(1, Ordering::SeqCst);
                    let (commands_tx, commands_rx) = mpsc::channel(COMMAND_QUEUE);
                    shared.commands.lock().unwrap().push(commands_tx);
                    tokio::spawn(serve_connection(stream, shared.clone(), commands_rx));
                }
            }
        });
        Self {
            endpoint: Channel::from_shared(format!("http://{addr}")).unwrap(),
            shared,
            accept_loop,
        }
    }

    pub(crate) fn endpoint(&self) -> Endpoint {
        self.endpoint.clone()
    }

    pub(crate) fn set_read_rows_mode(&self, mode: ReadRowsMode) {
        *self.shared.read_rows_mode.lock().unwrap() = mode;
    }

    /// TCP connections accepted so far.
    pub(crate) fn connections(&self) -> usize {
        self.shared.connections.load(Ordering::SeqCst)
    }

    /// The GOAWAY the client sent, if any.
    pub(crate) fn client_goaway(&self) -> Option<GoAway> {
        self.shared.client_goaway.lock().unwrap().clone()
    }

    /// Wait until at least `count` `ReadRows` requests have been held or ignored.
    pub(crate) async fn wait_for_unanswered_requests(&self, count: usize) {
        wait_until(
            &self.shared.unanswered_requests,
            count,
            "unanswered requests",
        )
        .await;
    }

    /// Wait until the client has reset at least `count` streams.
    pub(crate) async fn wait_for_resets(&self, count: usize) {
        wait_until(&self.shared.reset_streams, count, "stream resets").await;
    }

    /// Send a DATA frame on every held stream the client reset, then a PING. Returns once the
    /// client acknowledges the PING (so it has processed every DATA frame) or closes the
    /// connection.
    pub(crate) async fn flood_reset_streams(&self, payload_len: usize) {
        self.broadcast(|| Command::FloodResetStreams { payload_len })
            .await;
    }

    /// Finish every held stream the client has not reset, then wait for a PING round trip as in
    /// [`Self::flood_reset_streams`].
    pub(crate) async fn complete_held_streams(&self) {
        self.broadcast(|| Command::CompleteHeldStreams).await;
    }

    /// Answer every ignored request the client has since cancelled, then wait for a PING round
    /// trip as in [`Self::flood_reset_streams`].
    pub(crate) async fn answer_ignored_streams(&self, payload_len: usize) {
        self.broadcast(|| Command::AnswerIgnoredStreams { payload_len })
            .await;
    }

    /// Send a graceful GOAWAY on every connection with ignored requests, then wait for a PING
    /// round trip as in [`Self::flood_reset_streams`].
    pub(crate) async fn goaway_ignoring_connections(&self) {
        self.broadcast(|| Command::GoAwayIgnoringConnections).await;
    }

    /// Close every connection holding a stream the client has not reset, as a connection that
    /// drops does: no GOAWAY, then end of stream. Returns once the server has shut down its side
    /// of those connections; the client sees the close shortly after.
    pub(crate) async fn close_holding_connections(&self) {
        self.broadcast(|| Command::CloseHoldingConnections).await;
    }

    async fn broadcast(&self, command: impl Fn() -> Command) {
        let pending: Vec<_> = self
            .shared
            .commands
            .lock()
            .unwrap()
            .iter()
            .filter_map(|commands| {
                let (done, finished) = oneshot::channel();
                match commands.try_send((command(), done)) {
                    Ok(()) => Some(finished),
                    Err(mpsc::error::TrySendError::Closed(_)) => None,
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        panic!("raw HTTP/2 connection has {COMMAND_QUEUE} unprocessed commands")
                    }
                }
            })
            .collect();
        for finished in pending {
            // A receive error means the connection closed, which ends the round trip too.
            let _ = tokio::time::timeout(Duration::from_secs(10), finished)
                .await
                .expect("raw HTTP/2 connection did not acknowledge the PING");
        }
    }
}

impl Drop for RawH2Server {
    fn drop(&mut self) {
        self.accept_loop.abort();
    }
}

async fn wait_until(counter: &AtomicUsize, count: usize, what: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while counter.load(Ordering::SeqCst) < count {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {count} {what}"));
}

async fn serve_connection(
    stream: TcpStream,
    shared: Arc<Shared>,
    mut commands: mpsc::Receiver<(Command, oneshot::Sender<()>)>,
) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let (mut reader, mut writer) = stream.into_split();
    let mut preface = [0u8; CLIENT_PREFACE_LEN];
    reader.read_exact(&mut preface).await?;

    // Frames are read on their own task so that waiting for one never races with a command.
    let (frames_tx, mut frames) = mpsc::channel(FRAME_QUEUE);
    tokio::spawn(async move {
        while let Ok(frame) = read_frame(&mut reader).await {
            if frames_tx.send(frame).await.is_err() {
                return;
            }
        }
    });

    let mut out = BytesMut::new();
    let mut settings = BytesMut::new();
    settings.put_u16(SETTINGS_MAX_CONCURRENT_STREAMS);
    settings.put_u32(shared.max_concurrent_streams);
    put_frame(&mut out, FRAME_SETTINGS, 0, 0, &settings);
    put_frame(
        &mut out,
        FRAME_WINDOW_UPDATE,
        0,
        0,
        &MAX_WINDOW_INCREMENT.to_be_bytes(),
    );
    writer.write_all_buf(&mut out).await?;

    let mut streams = Streams::default();
    let mut request_bodies: HashMap<u32, BytesMut> = HashMap::new();
    let mut ping_round_trip: Option<([u8; 8], oneshot::Sender<()>)> = None;
    let mut next_ping: u64 = 0;
    loop {
        tokio::select! {
            frame = frames.recv() => {
                let Some(frame) = frame else {
                    return Ok(());
                };
                match frame.kind {
                    FRAME_SETTINGS if frame.flags & FLAG_ACK == 0 => {
                        put_frame(&mut out, FRAME_SETTINGS, FLAG_ACK, 0, &[]);
                    }
                    FRAME_PING if frame.flags & FLAG_ACK == 0 => {
                        put_frame(&mut out, FRAME_PING, FLAG_ACK, 0, &frame.payload);
                    }
                    FRAME_PING => {
                        if ping_round_trip
                            .as_ref()
                            .is_some_and(|(payload, _)| payload[..] == frame.payload[..])
                            && let Some((_, done)) = ping_round_trip.take()
                        {
                            let _ = done.send(());
                        }
                    }
                    FRAME_HEADERS => {
                        request_bodies.insert(frame.stream_id, BytesMut::new());
                        if frame.flags & FLAG_END_STREAM != 0 {
                            respond(&shared, &mut streams, &mut out, frame.stream_id, &[]);
                        }
                    }
                    FRAME_DATA => {
                        let body = request_bodies.entry(frame.stream_id).or_default();
                        body.extend_from_slice(&frame.payload);
                        if frame.flags & FLAG_END_STREAM != 0 {
                            let body = request_bodies.remove(&frame.stream_id).unwrap_or_default();
                            respond(&shared, &mut streams, &mut out, frame.stream_id, &body);
                        }
                    }
                    FRAME_RST_STREAM => {
                        if streams.reset.insert(frame.stream_id) {
                            shared.reset_streams.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    FRAME_GOAWAY => {
                        let error_code = u32::from_be_bytes(frame.payload[4..8].try_into().unwrap());
                        *shared.client_goaway.lock().unwrap() = Some(GoAway {
                            error_code,
                            debug_data: frame.payload.slice(8..),
                        });
                    }
                    _ => {}
                }
            }
            command = commands.recv() => {
                let Some((command, done)) = command else {
                    return Ok(());
                };
                match command {
                    Command::FloodResetStreams { payload_len } => {
                        let payload = vec![0u8; payload_len];
                        for stream_id in streams.held.keys() {
                            if streams.reset.contains(stream_id) {
                                put_frame(&mut out, FRAME_DATA, 0, *stream_id, &payload);
                            }
                        }
                    }
                    Command::CompleteHeldStreams => {
                        let Streams { held, reset, .. } = &mut streams;
                        held.retain(|stream_id, keys| {
                            if reset.contains(stream_id) {
                                return true;
                            }
                            put_read_rows_body(&mut out, *stream_id, keys);
                            false
                        });
                    }
                    Command::AnswerIgnoredStreams { payload_len } => {
                        let payload = vec![0u8; payload_len];
                        let Streams { ignored, reset, .. } = &mut streams;
                        ignored.retain(|stream_id| {
                            if !reset.contains(stream_id) {
                                return true;
                            }
                            put_response_headers(&mut out, *stream_id);
                            put_frame(&mut out, FRAME_DATA, 0, *stream_id, &payload);
                            false
                        });
                    }
                    Command::GoAwayIgnoringConnections => {
                        if !streams.ignored.is_empty() {
                            let mut goaway = BytesMut::new();
                            goaway.put_u32(0);
                            goaway.put_u32(NO_ERROR);
                            goaway.put_slice(b"load_shed");
                            put_frame(&mut out, FRAME_GOAWAY, 0, 0, &goaway);
                            streams.ignored.clear();
                        }
                    }
                    Command::CloseHoldingConnections => {
                        if streams.held.keys().any(|id| !streams.reset.contains(id)) {
                            // Dropping the write half sends FIN; the reader task stops once it
                            // can no longer hand frames to this task.
                            return Ok(());
                        }
                    }
                }
                next_ping += 1;
                let payload = next_ping.to_be_bytes();
                put_frame(&mut out, FRAME_PING, 0, 0, &payload);
                ping_round_trip = Some((payload, done));
            }
        }
        // A client that closes its connection may have sent a GOAWAY first; keep reading until
        // the connection ends so that every frame it sent is recorded.
        if writer.write_all_buf(&mut out).await.is_err() {
            out.clear();
        }
    }
}

/// Answer a complete request on `stream_id` whose body is `body`.
fn respond(
    shared: &Shared,
    streams: &mut Streams,
    out: &mut BytesMut,
    stream_id: u32,
    body: &[u8],
) {
    // A gRPC message is a 5-byte prefix (compression flag, big-endian length) and the message.
    let message = body.get(5..).unwrap_or_default();
    if message.is_empty() {
        put_response_headers(out, stream_id);
        put_frame(out, FRAME_DATA, 0, stream_id, &grpc_message(&[]));
        put_trailers(out, stream_id);
        return;
    }
    let request = ReadRowsRequest::decode(message).expect("ReadRows request");
    let keys = request.rows.map(|rows| rows.row_keys).unwrap_or_default();
    let mode = *shared.read_rows_mode.lock().unwrap();
    match mode {
        ReadRowsMode::Respond => {
            put_response_headers(out, stream_id);
            put_read_rows_body(out, stream_id, &keys);
            return;
        }
        ReadRowsMode::ServiceUnavailable => {
            put_frame(
                out,
                FRAME_HEADERS,
                FLAG_END_HEADERS | FLAG_END_STREAM,
                stream_id,
                &service_unavailable_headers(),
            );
            return;
        }
        ReadRowsMode::HoldAfterHeaders => {
            put_response_headers(out, stream_id);
            streams.held.insert(stream_id, keys);
        }
        ReadRowsMode::Ignore => {
            streams.ignored.insert(stream_id);
        }
    }
    shared.unanswered_requests.fetch_add(1, Ordering::SeqCst);
}

fn put_response_headers(out: &mut BytesMut, stream_id: u32) {
    put_frame(
        out,
        FRAME_HEADERS,
        FLAG_END_HEADERS,
        stream_id,
        &response_headers(),
    );
}

fn put_read_rows_body(out: &mut BytesMut, stream_id: u32, keys: &[Bytes]) {
    let response = ReadRowsResponse {
        chunks: keys
            .iter()
            .map(|key| CellChunk {
                row_key: key.clone(),
                family_name: Some(crate::tables::FAMILY.to_string()),
                qualifier: Some(b"v".to_vec()),
                timestamp_micros: 0,
                labels: vec![],
                value: key.clone(),
                value_size: 0,
                row_status: Some(RowStatus::CommitRow(true)),
            })
            .collect(),
        last_scanned_row_key: Bytes::new(),
        request_stats: None,
    };
    put_frame(
        out,
        FRAME_DATA,
        0,
        stream_id,
        &grpc_message(&response.encode_to_vec()),
    );
    put_trailers(out, stream_id);
}

/// HPACK block for `:status: 200` and `content-type: application/grpc`, using no dynamic table
/// entries so the client's decoder state stays in sync without tracking it here.
fn response_headers() -> Vec<u8> {
    // Indexed field 8 (`:status: 200`), then a literal without indexing whose name is static
    // entry 31 (`content-type`): prefix 0x0f plus 16 encodes 31 in the 4-bit prefix.
    let mut block = vec![0x88, 0x0f, 0x10, 16];
    block.extend_from_slice(b"application/grpc");
    block
}

/// HPACK block for `:status: 503` alone. 503 has no static table entry, so it is a literal
/// without indexing whose name is static entry 8 (`:status`).
fn service_unavailable_headers() -> Vec<u8> {
    let mut block = vec![0x08, 3];
    block.extend_from_slice(b"503");
    block
}

fn put_trailers(out: &mut BytesMut, stream_id: u32) {
    // Literal without indexing, new name `grpc-status`, value `0`.
    let mut block = vec![0x00, 11];
    block.extend_from_slice(b"grpc-status");
    block.extend_from_slice(&[1, b'0']);
    put_frame(
        out,
        FRAME_HEADERS,
        FLAG_END_HEADERS | FLAG_END_STREAM,
        stream_id,
        &block,
    );
}

fn grpc_message(message: &[u8]) -> Vec<u8> {
    let mut framed = Vec::with_capacity(5 + message.len());
    framed.push(0);
    framed.extend_from_slice(&(message.len() as u32).to_be_bytes());
    framed.extend_from_slice(message);
    framed
}

fn put_frame(out: &mut BytesMut, kind: u8, flags: u8, stream_id: u32, payload: &[u8]) {
    assert!(
        payload.len() <= MAX_FRAME_SIZE,
        "a {}-byte frame exceeds the client's maximum frame size",
        payload.len()
    );
    out.put_slice(&(payload.len() as u32).to_be_bytes()[1..]);
    out.put_u8(kind);
    out.put_u8(flags);
    out.put_u32(stream_id);
    out.put_slice(payload);
}

async fn read_frame(reader: &mut OwnedReadHalf) -> io::Result<RawFrame> {
    let mut header = [0u8; 9];
    reader.read_exact(&mut header).await?;
    let len = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;
    Ok(RawFrame {
        kind: header[3],
        flags: header[4],
        stream_id: u32::from_be_bytes(header[5..9].try_into().unwrap()) & 0x7fff_ffff,
        payload: Bytes::from(payload),
    })
}
