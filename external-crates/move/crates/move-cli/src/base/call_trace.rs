// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! # Call Trace Visualization Module
//!
//! This module provides functionality to visualize Move execution traces as Mermaid diagrams.
//! It processes trace files generated from Move and converts them into human-readable
//! diagrams that show the flow of function calls and their relationships.
//!
//! ## Features
//!
//! - **Flowchart Generation**: Creates hierarchical call graphs showing function call relationships
//! - **Sequence Diagram Generation**: Produces UML-style sequence diagrams with optimized participant ordering
//! - **Call Frequency Analysis**: Analyzes and optimizes participant positioning based on interaction patterns
//! - **Module Filtering**: Supports regex-based filtering to focus on specific modules
//!
//! ## Diagram Types
//!
//! 1. **Flowchart**: Shows the hierarchical structure of function calls as a tree
//! 2. **Sequence Diagram**: Displays the temporal order of function calls with participants grouped by package
//!
//! ## Optimization
//!
//! The sequence diagram generator includes an intelligent ordering algorithm that positions
//! frequently interacting functions closer together to minimize line crossings and improve
//! readability when generating sequence diagrams.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use move_core_types::account_address::AccountAddress;
use move_trace_format::format::{Frame, MoveTraceReader, TraceEvent, TraceIndex, TraceValue};

const ENTRY_NAME: &str = "Entry";

/// Supported diagram types for visualization
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DiagramType {
    /// Hierarchical flowchart showing call relationships
    Flowchart,
    /// UML-style sequence diagram showing temporal call order
    Sequence,
}

/// Generates a Mermaid diagram from a Move trace file. Supports both call graphs and sequence
/// diagrams.
#[derive(Parser)]
#[clap(name = "call-trace")]
pub struct CallTrace {
    /// The path to the trace file
    #[clap(short = 'i', long = "input")]
    pub input: PathBuf,

    /// The name of the file to output the diagram to. If not provided, outputs to stdout.
    #[clap(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// The type of diagram to generate
    #[clap(short = 't', long = "type", value_enum, default_value = "sequence")]
    pub diagram_type: DiagramType,

    /// Filter functions by module pattern (regex)
    #[clap(long = "filter-module")]
    pub filter_module: Option<String>,

    #[clap(long = "long-names", default_value = "false")]
    pub long_names: bool,
}

/// Represents information about a single function call in the trace
struct CallInfo {
    #[allow(dead_code)]
    frame_id: TraceIndex,
    /// The frame containing function metadata
    frame: Frame,
    #[allow(dead_code)]
    parameters: Vec<TraceValue>,
    /// Return values from this function call
    return_values: Option<Vec<TraceValue>>,
    /// Child function calls made by this function
    children: Vec<CallInfo>,
}

/// Fully qualified identifier for a function, including package, module, and function name
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct FqID {
    /// Package/version identifier
    version_id: AccountAddress,
    /// Module name within the package
    module_name: String,
    /// Function name within the module
    function_name: String,
}

impl CallTrace {
    /// Main entry point for the call trace command.
    /// Reads the trace file, builds the call tree, and generates the requested diagram type.
    pub fn execute(&self) -> Result<()> {
        let fh = File::open(&self.input)?;
        let reader = MoveTraceReader::new(fh)?;

        let calls = self.build_call_tree(reader)?;

        let diagram = match self.diagram_type {
            DiagramType::Flowchart => self.generate_flowchart(&calls)?,
            DiagramType::Sequence => self.generate_sequence_diagram(&calls)?,
        };

        match &self.output {
            Some(path) => {
                let mut file = File::create(path)?;
                file.write_all(diagram.as_bytes())?;
                println!("Mermaid diagram written to: {}", path.display());
            }
            None => {
                println!("{}", diagram);
            }
        }

        Ok(())
    }

    /// Builds a hierarchical tree of function calls from the trace events.
    ///
    /// This function processes trace events sequentially, maintaining a stack to track
    /// the current call hierarchy. It handles OpenFrame and CloseFrame events to construct
    /// the complete call tree.
    ///
    /// # Arguments
    /// * `reader` - The trace file reader
    ///
    /// # Returns
    /// A vector of root-level CallInfo structures representing the top-level function calls
    fn build_call_tree(&self, reader: MoveTraceReader<File>) -> Result<Vec<CallInfo>> {
        let mut stack: Vec<CallInfo> = Vec::new();
        let mut root_calls: Vec<CallInfo> = Vec::new();
        let mut frame_map: BTreeMap<TraceIndex, usize> = BTreeMap::new();

        for event in reader {
            let event = event?;
            match event {
                TraceEvent::OpenFrame { frame, .. } => {
                    let frame_id = frame.frame_id;
                    let parameters = frame.parameters.clone();

                    let call_info = CallInfo {
                        frame_id,
                        frame: *frame,
                        parameters,
                        return_values: None,
                        children: Vec::new(),
                    };

                    if let Some(module_filter) = &self.filter_module {
                        let module_regex = regex::Regex::new(module_filter)?;
                        if !module_regex.is_match(&call_info.frame.module.short_str_lossless()) {
                            continue;
                        }
                    }

                    frame_map.insert(frame_id, stack.len());
                    stack.push(call_info);
                }
                TraceEvent::CloseFrame {
                    frame_id, return_, ..
                } => {
                    if let Some(&stack_idx) = frame_map.get(&frame_id) {
                        if stack_idx < stack.len() {
                            let mut call = stack.remove(stack_idx);
                            call.return_values = Some(return_);

                            // Update frame_map indices
                            for idx in stack_idx..stack.len() {
                                if let Some(fid) = frame_map
                                    .iter()
                                    .find(|(_, v)| **v == idx + 1)
                                    .map(|(k, _)| *k)
                                {
                                    frame_map.insert(fid, idx);
                                }
                            }
                            frame_map.remove(&frame_id);

                            if stack.is_empty() {
                                root_calls.push(call);
                            } else {
                                stack.last_mut().unwrap().children.push(call);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(root_calls)
    }

    /// Generates a Mermaid flowchart diagram from the call tree.
    ///
    /// Creates a hierarchical visualization where each function is a node and edges
    /// represent call relationships.
    ///
    /// # Arguments
    /// * `calls` - The root calls of the call tree
    ///
    /// # Returns
    /// A string containing the Mermaid flowchart syntax
    fn generate_flowchart(&self, calls: &[CallInfo]) -> Result<String> {
        let mut output = String::from("flowchart TD\n");
        let mut node_counter = 0;

        for call in calls {
            self.add_flowchart_nodes(&mut output, call, &mut node_counter)?;
        }

        output.push('\n');
        Ok(output)
    }

    /// Recursively adds nodes to the flowchart for a call and its children.
    ///
    /// # Arguments
    /// * `output` - The string builder for the diagram
    /// * `call` - The current call to process
    /// * `counter` - Node ID counter for unique identification
    ///
    /// # Returns
    /// The node ID of the created node
    fn add_flowchart_nodes(
        &self,
        output: &mut String,
        call: &CallInfo,
        counter: &mut usize,
    ) -> Result<String> {
        let node_id = format!("node{}", counter);
        *counter += 1;

        let label = self.format_function_name(&call.frame);
        output.push_str(&format!("    {}[\"{}\"]\n", node_id, label));

        for child in &call.children {
            let child_id = self.add_flowchart_nodes(output, child, counter)?;
            if !child_id.is_empty() {
                output.push_str(&format!("    {} --> {}\n", node_id, child_id));
            }
        }

        Ok(node_id)
    }

    /// Generates a Mermaid sequence diagram from the call tree.
    ///
    /// Creates a UML-style sequence diagram showing the temporal order of function calls.
    /// Participants are grouped by package and ordered to minimize line crossings based on
    /// call frequency analysis.
    ///
    /// # Arguments
    /// * `calls` - The root calls of the call tree
    ///
    /// # Returns
    /// A string containing the Mermaid sequence diagram syntax
    fn generate_sequence_diagram(&self, calls: &[CallInfo]) -> Result<String> {
        let mut output = String::from("sequenceDiagram\n");
        let mut participant_packages = std::collections::BTreeMap::new();
        let mut participants: BTreeMap<FqID, usize> = BTreeMap::new();
        let mut participant_counter = 0;

        // Collect all unique participants
        self.collect_participants(calls, &mut participant_packages, &mut participant_counter);

        // Analyze call frequencies to optimize participant ordering
        let call_frequencies = self.analyze_call_frequencies(calls);
        let optimized_packages =
            self.optimize_participant_ordering(participant_packages, &call_frequencies);

        output.push_str("    autonumber\n");

        // Declare participants with optimized ordering
        for (i, (group, group_members)) in optimized_packages.iter().enumerate() {
            output.push_str(&format!(
                "    box {} Package 0x{}\n",
                Self::simple_color(i + 1, 0.1),
                group.short_str_lossless()
            ));
            for (participant, i) in group_members {
                output.push_str(&format!(
                    "    participant P{i} as \"{}::{}\"\n",
                    participant.module_name, participant.function_name
                ));
                participants.insert(participant.clone(), *i);
            }
            output.push_str("    end\n");
        }

        output.push_str(&format!(
            "    participant P{participant_counter} as {ENTRY_NAME}\n"
        ));
        participants.insert(FqID::entry_point(), participant_counter);

        output.push('\n');

        // Generate sequence
        for call in calls {
            self.add_sequence_calls(&mut output, &participants, call, &FqID::entry_point())?;
        }

        output.push('\n');
        Ok(output)
    }

    /// Recursively collects all unique function participants from the call tree.
    ///
    /// Groups participants by their package (AccountAddress) for organized display.
    ///
    /// # Arguments
    /// * `calls` - The calls to process
    /// * `participants` - Map of package to functions within that package
    /// * `participant_counter` - Counter for assigning unique IDs to participants
    fn collect_participants(
        &self,
        calls: &[CallInfo],
        participants: &mut BTreeMap<AccountAddress, BTreeMap<FqID, usize>>,
        participant_counter: &mut usize,
    ) {
        for call in calls {
            participants
                .entry(call.frame.version_id)
                .or_default()
                .insert(self.get_function_name(&call.frame), *participant_counter);
            *participant_counter += 1;
            self.collect_participants(&call.children, participants, participant_counter);
        }
    }

    /// Analyzes the frequency of calls between function pairs.
    ///
    /// This information is used to optimize participant ordering in sequence diagrams.
    ///
    /// # Arguments
    /// * `calls` - The root calls to analyze
    ///
    /// # Returns
    /// A map from function pairs to their call frequency
    fn analyze_call_frequencies(&self, calls: &[CallInfo]) -> BTreeMap<(FqID, FqID), usize> {
        let mut frequencies = BTreeMap::new();
        self.count_call_frequencies(calls, &FqID::entry_point(), &mut frequencies);
        frequencies
    }

    /// Recursively counts call frequencies between function pairs.
    ///
    /// Records bidirectional call relationships to capture full interaction patterns.
    ///
    /// # Arguments
    /// * `calls` - The calls to process
    /// * `caller` - The current caller in the call stack
    /// * `frequencies` - Accumulator for frequency counts
    fn count_call_frequencies(
        &self,
        calls: &[CallInfo],
        caller: &FqID,
        frequencies: &mut BTreeMap<(FqID, FqID), usize>,
    ) {
        for call in calls {
            let callee = self.get_function_name(&call.frame);
            *frequencies
                .entry((caller.clone(), callee.clone()))
                .or_insert(0) += 1;
            *frequencies
                .entry((callee.clone(), caller.clone()))
                .or_insert(0) += 1;
            self.count_call_frequencies(&call.children, &callee, frequencies);
        }
    }

    /// Optimizes the ordering of participants within each package to minimize line crossings.
    ///
    /// Uses call frequency analysis to position frequently interacting functions closer together.
    /// This improves diagram readability by reducing the visual complexity of call arrows.
    ///
    /// # Arguments
    /// * `packages` - Functions grouped by package
    /// * `call_frequencies` - Frequency of calls between function pairs
    ///
    /// # Returns
    /// Optimally ordered participants for each package
    fn optimize_participant_ordering(
        &self,
        packages: BTreeMap<AccountAddress, BTreeMap<FqID, usize>>,
        call_frequencies: &BTreeMap<(FqID, FqID), usize>,
    ) -> BTreeMap<AccountAddress, Vec<(FqID, usize)>> {
        let mut optimized = BTreeMap::new();

        for (package_id, members) in packages {
            let mut ordered_members: Vec<FqID> = members.keys().cloned().collect();

            // Calculate interaction scores for each participant within the package
            let members_for_scoring = ordered_members.clone();
            ordered_members.sort_by_cached_key(|participant| {
                let mut position_weight = 0i64;

                for other in members_for_scoring.iter() {
                    if participant == other {
                        continue;
                    }

                    let freq = call_frequencies
                        .get(&(participant.clone(), other.clone()))
                        .copied()
                        .unwrap_or(0) as i64;

                    position_weight += freq;
                }

                // Return negative score so higher interaction participants are sorted first
                -position_weight
            });

            // Now optimize the ordering using a greedy approach
            let optimized_order = self.greedy_optimize_order(&ordered_members, call_frequencies);

            // Assign new indices based on optimized order
            let mut ordered_with_indices = Vec::new();
            for (new_idx, participant) in optimized_order.into_iter().enumerate() {
                let original_idx = members.get(&participant).copied().unwrap_or(new_idx);
                ordered_with_indices.push((participant, original_idx));
            }

            optimized.insert(package_id, ordered_with_indices);
        }

        optimized
    }

    /// Applies a greedy algorithm to order participants based on their interaction strength.
    ///
    /// Starts with the most connected participant and iteratively adds the participant
    /// with the strongest connections to already-placed participants.
    ///
    /// # Arguments
    /// * `participants` - The participants to order
    /// * `call_frequencies` - Frequency of calls between participants
    ///
    /// # Returns
    /// Optimally ordered list of participants
    fn greedy_optimize_order(
        &self,
        participants: &[FqID],
        call_frequencies: &BTreeMap<(FqID, FqID), usize>,
    ) -> Vec<FqID> {
        if participants.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        let mut remaining: Vec<FqID> = participants.to_vec();

        // Start with the participant that has the most interactions
        let (start_idx, _) = remaining
            .iter()
            .enumerate()
            .max_by_key(|(_, p)| {
                participants
                    .iter()
                    .filter(|other| *other != *p)
                    .map(|other| {
                        call_frequencies
                            .get(&((*p).clone(), other.clone()))
                            .copied()
                            .unwrap_or(0)
                    })
                    .sum::<usize>()
            })
            .unwrap_or((0, &remaining[0]));

        result.push(remaining.remove(start_idx));

        // Greedily add the participant with the strongest connection to the current group
        while !remaining.is_empty() {
            let (next_idx, _) = remaining
                .iter()
                .enumerate()
                .max_by_key(|(_, candidate)| {
                    result
                        .iter()
                        .map(|placed| {
                            call_frequencies
                                .get(&((*candidate).clone(), placed.clone()))
                                .copied()
                                .unwrap_or(0)
                        })
                        .sum::<usize>()
                })
                .unwrap_or((0, &remaining[0]));

            result.push(remaining.remove(next_idx));
        }

        result
    }

    /// Recursively adds sequence diagram calls for a function and its children.
    ///
    /// Generates the Mermaid syntax for function calls and returns in proper sequence.
    ///
    /// # Arguments
    /// * `output` - The string builder for the diagram
    /// * `participants` - Map of participants to their assigned IDs
    /// * `call` - The current call to process
    /// * `caller_name` - The name of the calling function
    fn add_sequence_calls(
        &self,
        output: &mut String,
        participants: &std::collections::BTreeMap<FqID, usize>,
        call: &CallInfo,
        caller_name: &FqID,
    ) -> Result<()> {
        let callee_name = self.get_function_name(&call.frame);
        let caller = format!("P{}", participants.get(caller_name).unwrap());
        let callee = format!("P{}", participants.get(&callee_name).unwrap());
        let call_msg = self.format_function_name(&call.frame);

        output.push_str(&format!("    {}->>+{}: {}\n", caller, callee, call_msg));

        // Process child calls
        for child in &call.children {
            self.add_sequence_calls(output, participants, child, &callee_name)?;
        }

        // Return
        output.push_str(&format!("    {}-->>-{}: return\n", callee, caller));
        Ok(())
    }

    /// Formats a function name for display based on the long_names setting.
    ///
    /// # Arguments
    /// * `frame` - The frame containing function information
    ///
    /// # Returns
    /// Formatted function name (either short or fully qualified)
    fn format_function_name(&self, frame: &Frame) -> String {
        if self.long_names {
            format!(
                "0x{}::{}::{}",
                frame.version_id.short_str_lossless(),
                frame.module.name(),
                frame.function_name
            )
        } else {
            frame.function_name.to_string()
        }
    }

    /// Extracts a fully qualified function identifier from a frame.
    ///
    /// # Arguments
    /// * `frame` - The frame containing function information
    ///
    /// # Returns
    /// A FqID struct with the complete function identification
    fn get_function_name(&self, frame: &Frame) -> FqID {
        FqID {
            version_id: frame.version_id,
            module_name: frame.module.name().to_string(),
            function_name: frame.function_name.to_string(),
        }
    }

    /// Generates a simple RGBA color based on an index.
    ///
    /// Used to color-code package boxes in sequence diagrams for visual distinction.
    ///
    /// # Arguments
    /// * `i` - Index used to generate the color
    /// * `alpha` - Alpha transparency value
    ///
    /// # Returns
    /// RGBA color string for Mermaid diagram styling
    fn simple_color(i: usize, alpha: f32) -> String {
        let r = (i * 70) % 256; // step by 70 to spread values
        let g = (i * 150) % 256;
        let b = (i * 220) % 256;
        format!("rgba({}, {}, {}, {:.2})", r, g, b, alpha)
    }
}

impl FqID {
    /// Creates a special FqID representing the entry point of execution.
    ///
    /// Used as the initial caller in sequence diagrams.
    pub fn entry_point() -> Self {
        FqID {
            version_id: AccountAddress::ZERO,
            module_name: ENTRY_NAME.to_string(),
            function_name: ENTRY_NAME.to_string(),
        }
    }
}

impl std::fmt::Display for FqID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "0x{}::{}::{}",
            self.version_id.short_str_lossless(),
            self.module_name,
            self.function_name
        )
    }
}
