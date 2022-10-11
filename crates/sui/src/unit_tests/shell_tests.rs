// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::env;
use std::sync::Arc;

use rustyline::completion::Completer;
use rustyline::history::History;
use rustyline::Context;

use sui_types::base_types::ObjectID;

use crate::shell::split_and_unescape;
use crate::shell::{
    substitute_env_variables, CacheKey, CommandStructure, CompletionCache, ShellHelper,
};

#[test]
fn test_completion_cache_key() {
    let mut cache = BTreeMap::new();
    cache.insert(CacheKey::flag("--id"), vec!["data".to_string()]);

    assert!(cache.contains_key(&CacheKey::new("some_command", "--id")));
    assert!(cache.contains_key(&CacheKey::flag("--id")));
    assert!(cache.contains_key(&CacheKey::new("", "--id")));
    assert!(!cache.contains_key(&CacheKey::flag("--address")))
}

#[test]
fn test_substitute_env_variables() {
    let random_id = ObjectID::random().to_string();
    env::set_var("OBJECT_ID", random_id.clone());

    let test_string_1 = "$OBJECT_ID".to_string();
    assert_eq!(random_id, substitute_env_variables(test_string_1));

    let test_string_2 = "$OBJECT_ID/SOME_DIRECTORY".to_string();
    assert_eq!(
        format!("{random_id}/SOME_DIRECTORY"),
        substitute_env_variables(test_string_2)
    );
    // Make sure variable with the same beginnings won't get substituted incorrectly
    let random_id_2 = ObjectID::random().to_string();
    env::set_var("OBJECT_ID_2", random_id_2.clone());
    let test_string_3 = "$OBJECT_ID_2".to_string();
    assert_eq!(random_id_2, substitute_env_variables(test_string_3));

    // Substitution will not happen if the variable is not found, and should not fail
    let test_string_4 = "$THIS_VARIABLE_DOES_NOT_EXISTS".to_string();
    assert_eq!(
        test_string_4.clone(),
        substitute_env_variables(test_string_4)
    );
}

#[test]
fn test_completer() {
    let helper = ShellHelper {
        command: CommandStructure {
            name: "test".to_string(),
            completions: vec!["command1".to_string(), "command2".to_string()],
            children: vec![
                CommandStructure {
                    name: "command1".to_string(),
                    completions: vec![
                        "--command_1_flag1".to_string(),
                        "--command_1_flag2".to_string(),
                        "--command_1_flag3".to_string(),
                    ],
                    children: vec![],
                },
                CommandStructure {
                    name: "command2".to_string(),
                    completions: vec![
                        "--command_2_flag1".to_string(),
                        "--command_2_flag2".to_string(),
                    ],
                    children: vec![],
                },
            ],
        },
        completion_cache: Arc::new(Default::default()),
    };

    let (start, candidates) = helper
        .complete("", 1, &Context::new(&History::new()))
        .unwrap();
    assert_eq!(0, start);
    assert_eq!(2, candidates.len());
    let candidates = candidates
        .iter()
        .map(|pair| pair.display.clone())
        .collect::<Vec<_>>();
    assert_eq!(vec!["command1", "command2"], candidates);

    let (start, candidates) = helper
        .complete("command", 1, &Context::new(&History::new()))
        .unwrap();
    assert_eq!(0, start);
    assert_eq!(2, candidates.len());
    let candidates = candidates
        .iter()
        .map(|pair| pair.display.clone())
        .collect::<Vec<_>>();
    assert_eq!(vec!["command1", "command2"], candidates);

    let (start, candidates) = helper
        .complete("command1 ", 1, &Context::new(&History::new()))
        .unwrap();
    assert_eq!(9, start);
    assert_eq!(3, candidates.len());
    let candidates = candidates
        .iter()
        .map(|pair| pair.display.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        vec![
            "--command_1_flag1",
            "--command_1_flag2",
            "--command_1_flag3"
        ],
        candidates
    );

    let (start, candidates) = helper
        .complete("command2 ", 1, &Context::new(&History::new()))
        .unwrap();
    assert_eq!(9, start);
    assert_eq!(2, candidates.len());
    let candidates = candidates
        .iter()
        .map(|pair| pair.display.clone())
        .collect::<Vec<_>>();
    assert_eq!(vec!["--command_2_flag1", "--command_2_flag2"], candidates);
}

#[test]
fn test_completer_with_cache() {
    let completion_cache: CompletionCache = Arc::new(Default::default());

    let helper = ShellHelper {
        command: CommandStructure {
            name: "test".to_string(),
            completions: vec!["command1".to_string(), "command2".to_string()],
            children: vec![
                CommandStructure {
                    name: "command1".to_string(),
                    completions: vec![
                        "--address".to_string(),
                        "--gas".to_string(),
                        "--other".to_string(),
                    ],
                    children: vec![],
                },
                CommandStructure {
                    name: "command2".to_string(),
                    completions: vec!["--address".to_string(), "--gas".to_string()],
                    children: vec![],
                },
            ],
        },
        completion_cache: completion_cache.clone(),
    };

    // CacheKey::flag applies to all flags regardless of the command name
    completion_cache.write().unwrap().insert(
        CacheKey::flag("--gas"),
        vec!["Gas1".to_string(), "Gas2".to_string(), "Gas3".to_string()],
    );

    let (_, candidates) = helper
        .complete("command1 --gas ", 1, &Context::new(&History::new()))
        .unwrap();
    assert_eq!(5, candidates.len());
    let candidates = candidates
        .iter()
        .map(|pair| pair.display.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        vec!["Gas1", "Gas2", "Gas3", "--address", "--other"],
        candidates
    );

    let (_, candidates) = helper
        .complete("command2 --gas ", 1, &Context::new(&History::new()))
        .unwrap();
    assert_eq!(4, candidates.len());
    let candidates = candidates
        .iter()
        .map(|pair| pair.display.clone())
        .collect::<Vec<_>>();
    assert_eq!(vec!["Gas1", "Gas2", "Gas3", "--address"], candidates);

    // CacheKey::new only apply the completion values to the flag with matching command name
    completion_cache.write().unwrap().insert(
        CacheKey::new("command1", "--address"),
        vec!["Address1".to_string(), "Address2".to_string()],
    );
    let (_, candidates) = helper
        .complete("command1 --address ", 1, &Context::new(&History::new()))
        .unwrap();
    assert_eq!(4, candidates.len());
    let candidates = candidates
        .iter()
        .map(|pair| pair.display.clone())
        .collect::<Vec<_>>();
    assert_eq!(vec!["Address1", "Address2", "--gas", "--other"], candidates);

    let (_, candidates) = helper
        .complete("command2 --address ", 1, &Context::new(&History::new()))
        .unwrap();
    assert_eq!(1, candidates.len());
    let candidates = candidates
        .iter()
        .map(|pair| pair.display.clone())
        .collect::<Vec<_>>();
    assert_eq!(vec!["--gas"], candidates);
}

#[test]
fn test_split_line() {
    let test = "create-example-nft --name \"test 1\" --description \"t e s t 2\"";
    let result = split_and_unescape(test).unwrap();
    assert_eq!(
        vec![
            "create-example-nft",
            "--name",
            "test 1",
            "--description",
            "t e s t 2"
        ],
        result
    );
}
