// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use graphviz_rust::cmd::Format;
use graphviz_rust::dot_generator::*;
use graphviz_rust::dot_structures::*;
use graphviz_rust::printer::DotPrinter;
use graphviz_rust::printer::PrinterContext;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::hash::Hash;
use std::io::{BufRead, BufReader, Write};
use std::{
    collections::{hash_map, BTreeMap, HashMap, HashSet},
    time::SystemTime,
};
use sui_types::base_types::ConciseableName;
use sui_types::committee::StakeUnit;
use tempfile::tempdir;
use tracing::error;

use sui_types::base_types::AuthorityName;

#[derive(Serialize, Deserialize, Hash, Eq, PartialEq, Debug, Clone)]
pub struct Event {
    pub name: Str,
    pub tags: BTreeMap<Str, TagValue>,
    id: u64,
}

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tags = self
            .tags
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "{}({})", self.name, tags)
    }
}

impl Event {
    pub fn new(name: Str, tags: BTreeMap<Str, TagValue>) -> Self {
        Self { name, tags, id: 0 }
    }

    pub fn into_event_and_metadata(mut self) -> (Self, EventMetadata) {
        let provided_stake = self
            .tags
            .remove(&"stake".into())
            .map(|s| s.try_into().unwrap());
        let required_stake = self
            .tags
            .remove(&"required_stake".into())
            .map(|s| s.try_into().unwrap());

        (
            self,
            EventMetadata {
                provided_stake,
                required_stake,
                ..Default::default()
            },
        )
    }

    pub fn maybe_set_source(&mut self, source: Source) {
        self.tags
            .entry("source".into())
            .or_insert_with(|| source.into());
    }

    pub fn canonicalize(&mut self) {
        let mut tags = BTreeMap::new();
        for (k, v) in self.tags.iter() {
            tags.insert(k.canonicalize(), v.clone());
        }
        self.tags = tags;
    }
}

#[derive(Serialize, Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(into = "String", try_from = "String")]
pub enum Str {
    Dynamic(String),
    Static(&'static str),
}

impl Str {
    fn canonicalize(&self) -> Str {
        match self {
            Str::Dynamic(value) => match value.as_str() {
                "source" => Str::Static("source"),
                "stake" => Str::Static("stake"),
                "required_stake" => Str::Static("required_stake"),
                _ => Str::Dynamic(value.clone()),
            },
            Str::Static(value) => Str::Static(value),
        }
    }
}

impl std::fmt::Display for Str {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Str::Dynamic(value) => write!(f, "{}", value),
            Str::Static(value) => write!(f, "{}", value),
        }
    }
}

impl<'de> Deserialize<'de> for Str {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Str::Dynamic(value))
    }
}

impl From<Str> for String {
    fn from(value: Str) -> Self {
        match value {
            Str::Dynamic(value) => value,
            Str::Static(value) => value.to_string(),
        }
    }
}

impl<'a> From<&'a Str> for String {
    fn from(value: &'a Str) -> Self {
        match value {
            Str::Dynamic(value) => value.clone(),
            Str::Static(value) => value.to_string(),
        }
    }
}

impl From<String> for Str {
    fn from(value: String) -> Self {
        Str::Dynamic(value)
    }
}

impl From<&'static str> for Str {
    fn from(value: &'static str) -> Self {
        Str::Static(value)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq)]
pub enum TagValue {
    NumU64(u64),
    Str(String),
    Source(Source),
}

impl std::fmt::Display for TagValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagValue::NumU64(value) => write!(f, "{}", value),
            TagValue::Str(value) => write!(f, "{}", value),
            TagValue::Source(value) => write!(f, "{}", value),
        }
    }
}

impl From<u64> for TagValue {
    fn from(value: u64) -> Self {
        TagValue::NumU64(value)
    }
}

impl From<String> for TagValue {
    fn from(value: String) -> Self {
        TagValue::Str(value)
    }
}

impl From<Source> for TagValue {
    fn from(value: Source) -> Self {
        TagValue::Source(value)
    }
}

impl TryFrom<TagValue> for StakeUnit {
    type Error = &'static str;

    fn try_from(value: TagValue) -> Result<Self, Self::Error> {
        match value {
            TagValue::NumU64(value) => Ok(value),
            _ => Err("expected NumU64"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum Source {
    Remote(AuthorityName),
    Local,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Remote(value) => write!(f, "{:?}", value.concise()),
            Source::Local => write!(f, "<local>"),
        }
    }
}

impl From<AuthorityName> for Source {
    fn from(value: AuthorityName) -> Self {
        Source::Remote(value)
    }
}

impl From<&'static str> for Source {
    fn from(value: &'static str) -> Self {
        assert_eq!(value, "local");
        Source::Local
    }
}

#[macro_export]
macro_rules! process_tag {
    ($tags:expr, source, $tag_value:expr) => {
        $tags.insert("source".into(), $crate::Source::from($tag_value).into());
    };

    ($tags:expr, $tag_name:ident, $tag_value:expr) => {
        $tags.insert(stringify!($tag_name).into(), $tag_value.into());
    };
}

// parse_event! parses
//
//     "receive_checkpoint_sig", {
//       seq = signature.summary.sequence_number,
//     }
//
// into an Event struct
#[macro_export]
macro_rules! parse_event {
    ($name:literal { $($tag_name:ident = $tag_value:expr),* $(,)? }) => {
        {
            let mut tags = std::collections::BTreeMap::new();
            $({
                $crate::process_tag!(tags, $tag_name, $tag_value);
            })*
            $crate::Event::new($name.into(), tags)
        }
    };
}

pub fn process_event(event: Event, cause: Option<Event>) {
    let event_json = serde_json::to_string(&event).unwrap();
    let cause_json = cause.map(|cause| serde_json::to_string(&cause).unwrap());
    //tracing::debug!(event = event_json, cause = cause_json, "processing event");
    tracing::debug!(
        "event {} caused_by {}",
        event_json,
        cause_json.unwrap_or("None".to_string())
    );
}

pub fn process_expect_event(event: Event) {
    let event_json = serde_json::to_string(&event).unwrap();
    tracing::debug!("expecting event {}", event_json);
}

#[macro_export]
macro_rules! event {
    ($name:literal { $($tag_name:ident = $tag_value:expr),* $(,)? } caused_by $cause:literal { $($cause_tag_name:ident = $cause_tag_value:expr),* $(,)? }) => {
        {
            let event = $crate::parse_event!($name { $($tag_name = $tag_value),* });
            let cause = $crate::parse_event!($cause { $($cause_tag_name = $cause_tag_value),* });

            $crate::process_event(event, Some(cause));
        }
    };

    ($name:literal { $($tag_name:ident = $tag_value:expr),* $(,)? }) => {
        {
            let event = $crate::parse_event!($name { $($tag_name = $tag_value),* });
            $crate::process_event(event, None);
        }
    };
}

#[macro_export]
macro_rules! expect_event {
    ($name:literal { $($tag_name:ident = $tag_value:expr),* $(,)? }) => {
        {
            let event = $crate::parse_event!($name { $($tag_name = $tag_value),* });
            $crate::process_expect_event(event);
        }
    };
}

#[macro_export]
macro_rules! if_events_enabled {
    // Just pass through any stream of tokens. Eventually this will allow us to enable/disable
    // event-related code either at compile time or at runtime.
    ({ $($tokens:tt)* }) => {
        $($tokens)*
    };
}

#[derive(Debug, Clone, Default)]
pub struct EventMetadata {
    /// How much stake does this event provide to events it causes?
    pub provided_stake: Option<StakeUnit>,

    /// How much stake is required to cause this event
    pub required_stake: Option<StakeUnit>,

    /// How much causal stake has been observed so far?
    pub observed_stake: StakeUnit,
    /// Map of all events that have provided stake
    pub staking_events: HashMap<Event, StakeUnit>,
}

impl EventMetadata {
    pub fn is_fully_staked(&self) -> bool {
        self.required_stake
            .map(|required| self.observed_stake >= required)
            .unwrap_or(true)
    }
}

struct Edge {
    cause: Event,
    effect: Event,
    edge: Arc<Mutex<EventMetadata>>,
}

#[derive(Default, Debug)]
struct AnalysisState {
    // causes that are waiting for the events they cause
    future_causes: HashMap<Event, EventMetadata>,

    // events for which an expected cause has not yet been observed
    waiting_causes: HashMap<
        Event,
        (
            Event,         // the cause we are expecting
            EventMetadata, // metadata for the event
        ),
    >,

    dangling_causes: HashSet<Event>,
    dangling_events: HashSet<Event>,
    graph: HashMap<(Event, Event)>,

    //graph: HashSet<(Event, Event)>,
    seen: HashSet<Event>,
}

impl AnalysisState {
    fn process_event(&mut self, source: Source, event: Event, cause: Option<Event>) {
        let (event, metadata) = event.into_event_and_metadata();
        event.maybe_set_source(source);

        // events can be emitted multiple times, but we only want to process them once
        if !self.seen.insert(event.clone()) {
            return;
        }

        // This is very confusing, because `event` can be a cause of other events,
        if let Some(cause) = cause {
            let mut entry = self.future_causes.entry(cause.clone());
            match &mut entry {
                hash_map::Entry::Occupied(e) => {
                    let metadata = e.get_mut();
                    if let Some(required_stake) = metadata.required_stake {
                        let staking_entry = metadata.staking_events.entry(cause.clone());
                        let hash_map::Entry::Vacant(mut staking_entry) = staking_entry else {
                            // cause cannot already be in staking_events because we de-dup events
                            panic!("cause cannot already be in staking_events");
                        };
                        let Some(provided_stake) = provided_stake else {
                            eprintln!(
                                "Error: event {} is expected to provide stake, but did not",
                                event
                            );
                            return;
                        };

                        staking_entry.insert(provided_stake);
                        metadata.observed_stake += provided_stake;
                        self.graph.insert((cause.clone(), event.clone()));
                    } else {
                        // if no stake is required, the event is caused immediately
                        e.remove();
                        self.graph.insert((cause.clone(), event.clone()));
                    };
                }

                // we are expecting the cause to happen at some point,
                // but it hasn't happened yet. When it does happen, a graph
                // edge will be inserted
                hash_map::Entry::Vacant(_) => {
                    self.waiting_causes.insert(cause.clone(), event.clone());
                }
            }
        }

        if let Some(e) = self.waiting_causes.remove(&event) {
            // some other event was waiting for this event to happen
            self.graph.insert((event.clone(), e.clone()));
        }

        // this event may cause other things to happen later
        self.future_causes.insert(event, metadata);
    }

    // Dump any causes that have never been consumed, and any expected events
    // that have never been caused.
    fn dump_waiting(&self) {
        println!("Events that never caused anything:");
        for e in &self.future_causes {
            println!("-- {}", e);
        }

        println!("Expected causes that never happened:");
        for e in &self.waiting_causes {
            println!("-- {}", &e.0);
        }
    }

    fn dump_graph(&self) {
        let mut seen = HashSet::new();
        let mut graph = graph!(strict di id!("G"));
        graph.add_stmt(GraphAttributes::new("graph", vec![attr!("rankdir", "LR")]).into());

        let mut node_ids = HashMap::new();

        let mut next_id = 0u64;
        let mut get_id = |event: &Event| -> u64 {
            if let Some(id) = node_ids.get(event) {
                *id
            } else {
                let id = next_id;
                node_ids.insert(event.clone(), id);
                next_id += 1;
                id
            }
        };

        fn make_node<T: Into<String>>(id: u64, label: T) -> Node {
            let label: String = label.into();
            node!(id; attr!("label", label), attr!("fontname", "Courier"))
        }

        for (cause, event) in &self.graph {
            if seen.insert(cause) {
                let id = get_id(cause);
                let node = make_node(id, &cause.name);
                graph.add_stmt(node.into());
            }
            if seen.insert(event) {
                let id = get_id(event);
                let node = make_node(id, &event.name);
                graph.add_stmt(node.into());
            }
        }

        for (cause, event) in &self.graph {
            let cause_id = node_id!(get_id(cause));
            let event_id = node_id!(get_id(event));
            graph.add_stmt(edge!(cause_id => event_id).into());
        }

        println!("{}", graph.print(&mut PrinterContext::default()));

        let graph_pdf = graphviz_rust::exec(
            graph,
            &mut PrinterContext::default(),
            vec![Format::Pdf.into()],
        )
        .unwrap();

        // write graph to tempfile, use open to open it
        let dir = tempdir().unwrap();

        let dotfile = dir.path().join("graph.pdf");
        dbg!(&dotfile);
        let mut file = std::fs::File::create(&dotfile).unwrap();
        file.write_all(&graph_pdf).unwrap();
        std::process::Command::new("open")
            .arg(&dotfile)
            .output()
            .unwrap();
        Box::leak(Box::new(dir));
    }
}

fn parse_event(json_data: &str) -> Option<Event> {
    let event = if json_data == "None" {
        None
    } else {
        Some(serde_json::from_str::<Event>(json_data).unwrap())
    };
    event.map(|mut e| {
        e.canonicalize();
        e
    })
}

pub fn analyze_log(
    input: impl std::io::Read,
    output: impl std::io::Write,
    target_name: Option<&str>,
) {
    let target_name = target_name.unwrap_or("sui_causality_log:");
    let input = BufReader::new(input);

    let log_regex =
        Regex::new(r#"(.*) (\w+) sui_causality_log: event (.*) caused_by (.*)"#).unwrap();

    let mut state = AnalysisState::default();

    // process every line in the input
    for line in input.lines() {
        let line = line.unwrap();
        let line = strip_ansi_escapes::strip_str(line).to_owned();

        let Some(captures) = log_regex.captures(&line) else {
            continue;
        };

        let [timestamp, _, event_str, cause_str] = captures.extract().1;
        let event_json = event_str.to_owned();
        let cause_json = cause_str.to_owned();

        // timestamp is like 2024-05-15T20:44:21.829176Z, parse to SystemTime
        let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(&timestamp) else {
            error!("failed to parse timestamp: {}", timestamp);
            continue;
        };
        let timestamp: SystemTime = timestamp.into();

        let event = parse_event(&event_json).unwrap();
        let cause = parse_event(&cause_json);

        state.process_event(Source::Local, event, cause);
    }

    //dbg!(&state);
    //state.dump_graph();
    state.dump_waiting();
}
