// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::{rngs::ThreadRng, thread_rng, Rng};
use std::collections::HashSet;

/// This library provides two functions to generate
/// a random combination of an adjective
/// and a precious stone name as a well formatted
/// string, or a list of these strings.

/// A list of adjectives
const LEFT_NAMES: [&str; 108] = [
    "admiring",
    "adoring",
    "affectionate",
    "agitated",
    "amazing",
    "angry",
    "awesome",
    "beautiful",
    "blissful",
    "bold",
    "boring",
    "brave",
    "busy",
    "charming",
    "clever",
    "compassionate",
    "competent",
    "condescending",
    "confident",
    "cool",
    "cranky",
    "crazy",
    "dazzling",
    "determined",
    "distracted",
    "dreamy",
    "eager",
    "ecstatic",
    "elastic",
    "elated",
    "elegant",
    "eloquent",
    "epic",
    "exciting",
    "fervent",
    "festive",
    "flamboyant",
    "focused",
    "friendly",
    "frosty",
    "funny",
    "gallant",
    "gifted",
    "goofy",
    "gracious",
    "great",
    "happy",
    "hardcore",
    "heuristic",
    "hopeful",
    "hungry",
    "infallible",
    "inspiring",
    "intelligent",
    "interesting",
    "jolly",
    "jovial",
    "keen",
    "kind",
    "laughing",
    "loving",
    "lucid",
    "magical",
    "modest",
    "musing",
    "mystifying",
    "naughty",
    "nervous",
    "nice",
    "nifty",
    "nostalgic",
    "objective",
    "optimistic",
    "peaceful",
    "pedantic",
    "pensive",
    "practical",
    "priceless",
    "quirky",
    "quizzical",
    "recursing",
    "relaxed",
    "reverent",
    "romantic",
    "sad",
    "serene",
    "sharp",
    "silly",
    "sleepy",
    "stoic",
    "strange",
    "stupefied",
    "suspicious",
    "sweet",
    "tender",
    "thirsty",
    "trusting",
    "unruffled",
    "upbeat",
    "vibrant",
    "vigilant",
    "vigorous",
    "wizardly",
    "wonderful",
    "xenodochial",
    "youthful",
    "zealous",
    "zen",
];

const LEFT_LENGTH: usize = LEFT_NAMES.len();

/// A list of precious stones
const RIGHT_NAMES: [&str; 53] = [
    "agates",
    "alexandrite",
    "amber",
    "amethyst",
    "apatite",
    "avanturine",
    "axinite",
    "beryl",
    "beryl",
    "carnelian",
    "chalcedony",
    "chrysoberyl",
    "chrysolite",
    "chrysoprase",
    "coral",
    "corundum",
    "crocidolite",
    "cyanite",
    "cymophane",
    "diamond",
    "dichroite",
    "emerald",
    "epidote",
    "euclase",
    "felspar",
    "garnet",
    "heliotrope",
    "hematite",
    "hiddenite",
    "hypersthene",
    "idocrase",
    "jasper",
    "jet",
    "labradorite",
    "malachite",
    "moonstone",
    "obsidian",
    "opal",
    "pearl",
    "phenacite",
    "plasma",
    "prase",
    "quartz",
    "ruby",
    "sapphire",
    "sphene",
    "spinel",
    "spodumene",
    "sunstone",
    "topaz",
    "tourmaline",
    "turquois",
    "zircon",
];
const RIGHT_LENGTH: usize = RIGHT_NAMES.len();

/// Return a random name formatted as first-second from a list of strings.
///
/// The main purpose of this function is to generate random aliases for addresses.
pub fn random_name(conflicts: &HashSet<String>) -> String {
    let mut rng = thread_rng();
    // as long as the generated name is in the list of conflicts,
    // we try to find a different name that is not in the list yet
    loop {
        let output = generate(&mut rng);
        if !conflicts.contains(&output) {
            return output;
        }
    }
}

/// Return a unique collection of names.
pub fn random_names(mut conflicts: HashSet<String>, output_size: usize) -> Vec<String> {
    let mut names = Vec::with_capacity(output_size);
    names.resize_with(output_size, || {
        let name = random_name(&conflicts);
        conflicts.insert(name.clone());
        name
    });
    names
}

// Generate a random name as a pair from left and right string arrays
fn generate(rng: &mut ThreadRng) -> String {
    let left_idx = rng.gen_range(0..LEFT_LENGTH);
    let right_idx = rng.gen_range(0..RIGHT_LENGTH);
    format!(
        "{}-{}",
        LEFT_NAMES.get(left_idx).unwrap(),
        RIGHT_NAMES.get(right_idx).unwrap()
    )
}
