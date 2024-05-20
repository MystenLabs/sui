// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use graphviz_rust::cmd::Format;
use graphviz_rust::dot_generator::*;
use graphviz_rust::dot_structures::*;
use graphviz_rust::printer::DotPrinter;
use graphviz_rust::printer::PrinterContext;
use regex::Regex;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json;
use std::io::{BufRead, BufReader, Write};
use std::path::Display;
use std::sync::atomic::AtomicU64;
use std::{
    alloc::System,
    collections::{BTreeMap, HashMap, HashSet},
    time::{Instant, SystemTime},
};
use tempfile::tempfile;
use tracing::error;

use sui_types::base_types::AuthorityName;

#[derive(Serialize, Deserialize, Hash, Eq, PartialEq, Debug, Clone)]
pub struct Event {
    pub name: Str,
    pub tags: BTreeMap<Str, TagValue>,
    id: u64,
}

impl Event {
    pub fn new(name: Str, tags: BTreeMap<Str, TagValue>) -> Self {
        Self { name, tags, id: 0 }
    }
}

#[derive(Serialize, Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(into = "String", try_from = "String")]
pub enum Str {
    Dynamic(String),
    Static(&'static str),
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

pub struct EventMetadata {
    // when was it caused?
    pub time: Instant,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq)]
pub enum TagValue {
    NumU64(u64),
    Str(String),
    Source(Source),
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

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum Source {
    Remote(AuthorityName),
    Local,
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
macro_rules! if_events_enabled {
    // Just pass through any stream of tokens. Eventually this will allow us to enable/disable
    // event-related code either at compile time or at runtime.
    ({ $($tokens:tt)* }) => {
        $($tokens)*
    };
}

#[derive(Default, Debug)]
struct AnalysisState {
    // causes that are waiting for events
    future_causes: HashSet<Event>,

    // map from cause to future event
    waiting_causes: HashMap<Event, Event>,

    // events that are waiting for causes
    events: HashSet<Event>,

    graph: HashSet<(Event, Event)>,
}

impl AnalysisState {
    fn process_event(&mut self, event: Event, cause: Option<Event>) {
        // This is very confusing, because `event` can be a cause of other events,

        if let Some(cause) = cause {
            if let Some(c) = self.future_causes.get(&cause) {
                // the cause happened in the past
                self.graph.insert((c.clone(), event.clone()));
            } else {
                // we are expecting the cause to happen at some point,
                // but it hasn't happened yet. When it does happen, a graph
                // edge will be inserted
                self.waiting_causes.insert(cause.clone(), event.clone());
            }
        }

        if let Some(e) = self.waiting_causes.get(&event) {
            // some other event was waiting for this event to happen
            self.graph.insert((event.clone(), e.clone()));
        }

        // this event may cause other things to happen later
        self.future_causes.insert(event);
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
        let dir = tempfile::tempdir().unwrap();

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
    if json_data == "None" {
        None
    } else {
        Some(serde_json::from_str::<Event>(json_data).unwrap())
    }
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

        state.process_event(event, cause);
    }

    dbg!(&state);
    state.dump_graph();
}
