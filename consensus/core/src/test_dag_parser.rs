// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use consensus_config::AuthorityIndex;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1, take_while_m_n},
    character::complete::{char, digit1, multispace0, multispace1, space0, space1},
    combinator::{map_res, opt},
    multi::{many0, separated_list0},
    sequence::{delimited, preceded, terminated, tuple},
    IResult,
};

use crate::{
    block::{BlockRef, Round, Slot},
    context::Context,
    test_dag_builder::DagBuilder,
};

/// DagParser
///
/// Usage:
///
/// ```
/// let dag_str = "DAG {
///     Round 0 : { 4 },
///     Round 1 : { * },
///     Round 2 : { * },
///     Round 3 : { * },
///     Round 4 : {
///         A -> [-D3],
///         B -> [*],
///         C -> [*],
///         D -> [*],
///     },
///     Round 5 : {
///         A -> [*],
///         B -> [*],
///         C -> [A4],
///         D -> [A4],
///     },
///     Round 6 : { * },
///     Round 7 : { * },
///     Round 8 : { * },
///     }";
///
/// let (_, dag_builder) = parse_dag(dag_str).expect("Invalid dag"); // parse DAG DSL
/// dag_builder.print(); // print the parsed DAG
/// dag_builder.persist_all_blocks(dag_state.clone()); // persist all blocks to DagState
/// ```

pub(crate) fn parse_dag(dag_string: &str) -> IResult<&str, DagBuilder> {
    let (input, _) = tuple((tag("DAG"), multispace0, char('{')))(dag_string)?;

    let (mut input, num_authors) = parse_genesis(input)?;

    let context = Arc::new(Context::new_for_test(num_authors as usize).0);
    let mut dag_builder = DagBuilder::new(context);

    // Parse subsequent rounds
    loop {
        match parse_round(input, &dag_builder) {
            Ok((new_input, (round, connections))) => {
                dag_builder.layer_with_connections(connections, round);
                input = new_input
            }
            Err(nom::Err::Error(_)) | Err(nom::Err::Failure(_)) => break,
            Err(nom::Err::Incomplete(needed)) => return Err(nom::Err::Incomplete(needed)),
        }
    }
    let (input, _) = tuple((multispace0, char('}')))(input)?;

    Ok((input, dag_builder))
}

fn parse_round<'a>(
    input: &'a str,
    dag_builder: &DagBuilder,
) -> IResult<&'a str, (Round, Vec<(AuthorityIndex, Vec<BlockRef>)>)> {
    let (input, _) = tuple((multispace0, tag("Round"), space1))(input)?;
    let (input, round) = take_while1(|c: char| c.is_ascii_digit())(input)?;

    let (input, connections) = alt((
        |input| parse_fully_connected(input, dag_builder),
        |input| parse_specified_connections(input, dag_builder),
    ))(input)?;

    Ok((input, (round.parse().unwrap(), connections)))
}

fn parse_fully_connected<'a>(
    input: &'a str,
    dag_builder: &DagBuilder,
) -> IResult<&'a str, Vec<(AuthorityIndex, Vec<BlockRef>)>> {
    let (input, _) = tuple((
        space0,
        char(':'),
        space0,
        char('{'),
        space0,
        char('*'),
        space0,
        char('}'),
        opt(char(',')),
    ))(input)?;

    let ancestors = dag_builder.last_ancestors.clone();
    let connections = dag_builder
        .context
        .committee
        .authorities()
        .map(|authority| (authority.0, ancestors.clone()))
        .collect::<Vec<_>>();

    Ok((input, connections))
}

fn parse_specified_connections<'a>(
    input: &'a str,
    dag_builder: &DagBuilder,
) -> IResult<&'a str, Vec<(AuthorityIndex, Vec<BlockRef>)>> {
    let (input, _) = tuple((space0, char(':'), space0, char('{'), multispace0))(input)?;

    // parse specified connections
    // case 1: all authorities; [*]
    // case 2: specific included authorities; [A0, B0, C0]
    // case 3: specific excluded authorities;  [-A0]
    // case 4: mixed all authorities + specific included/excluded authorities; [*, A0]
    // TODO: case 5: byzantine case of multiple blocks per slot; [*]; timestamp=1
    let (input, authors_and_connections) = many0(parse_author_and_connections)(input)?;

    let mut output = Vec::new();
    for (author, connections) in authors_and_connections {
        let mut block_refs = HashSet::new();
        for connection in connections {
            if connection == "*" {
                block_refs.extend(dag_builder.last_ancestors.clone());
            } else if connection.starts_with('-') {
                let (input, _) = char('-')(connection)?;
                let (_, slot) = parse_slot(input)?;
                let stored_block_refs = get_blocks(slot, dag_builder);
                block_refs.extend(dag_builder.last_ancestors.clone());

                block_refs.retain(|ancestor| !stored_block_refs.contains(ancestor));
            } else {
                let input = connection;
                let (_, slot) = parse_slot(input)?;
                let stored_block_refs = get_blocks(slot, dag_builder);

                block_refs.extend(stored_block_refs);
            }
        }
        output.push((author, block_refs.into_iter().collect()));
    }

    let (input, _) = tuple((multispace0, char('}'), opt(char(','))))(input)?;

    Ok((input, output))
}

fn get_blocks(slot: Slot, dag_builder: &DagBuilder) -> Vec<BlockRef> {
    // note: special case for genesis blocks as they are cached separately
    let block_refs = if slot.round == 0 {
        dag_builder
            .genesis_block_refs()
            .into_iter()
            .filter(|block| Slot::from(*block) == slot)
            .collect::<Vec<_>>()
    } else {
        dag_builder
            .get_uncommitted_blocks_at_slot(slot)
            .iter()
            .map(|block| block.reference())
            .collect::<Vec<_>>()
    };
    block_refs
}

fn parse_author_and_connections(input: &str) -> IResult<&str, (AuthorityIndex, Vec<&str>)> {
    // parse author
    let (input, author) = preceded(
        multispace0,
        terminated(
            take_while1(|c: char| c.is_alphabetic()),
            preceded(opt(space0), tag("->")),
        ),
    )(input)?;

    // parse connections
    let (input, connections) = delimited(
        preceded(opt(space0), char('[')),
        separated_list0(tag(", "), parse_block),
        terminated(char(']'), opt(char(','))),
    )(input)?;
    let (input, _) = opt(multispace1)(input)?;
    Ok((
        input,
        (
            str_to_authority_index(author).expect("Invalid authority index"),
            connections,
        ),
    ))
}

fn parse_block(input: &str) -> IResult<&str, &str> {
    alt((
        map_res(tag("*"), |s: &str| Ok::<_, nom::error::ErrorKind>(s)),
        map_res(
            take_while1(|c: char| c.is_alphanumeric() || c == '-'),
            |s: &str| Ok::<_, nom::error::ErrorKind>(s),
        ),
    ))(input)
}

fn parse_genesis(input: &str) -> IResult<&str, u32> {
    let (input, num_authorities) = preceded(
        tuple((
            multispace0,
            tag("Round"),
            space1,
            char('0'),
            space0,
            char(':'),
            space0,
            char('{'),
            space0,
        )),
        |i| parse_authority_count(i),
    )(input)?;
    let (input, _) = tuple((space0, char('}'), opt(char(','))))(input)?;

    Ok((input, num_authorities))
}

fn parse_authority_count(input: &str) -> IResult<&str, u32> {
    let (input, num_str) = digit1(input)?;
    Ok((input, num_str.parse().unwrap()))
}

fn parse_slot(input: &str) -> IResult<&str, Slot> {
    let parse_authority = map_res(
        take_while_m_n(1, 1, |c: char| c.is_alphabetic() && c.is_uppercase()),
        |letter: &str| {
            Ok::<_, nom::error::ErrorKind>(
                str_to_authority_index(letter).expect("Invalid authority index"),
            )
        },
    );

    let parse_round = map_res(digit1, |digits: &str| digits.parse::<Round>());

    let mut parser = tuple((parse_authority, parse_round));

    let (input, (authority, round)) = parser(input)?;
    Ok((input, Slot::new(round, authority)))
}

// Helper function to convert a string representation (e.g., 'A' or '[26]') to an AuthorityIndex
fn str_to_authority_index(input: &str) -> Option<AuthorityIndex> {
    if input.starts_with('[') && input.ends_with(']') && input.len() > 2 {
        input[1..input.len() - 1]
            .parse::<u32>()
            .ok()
            .map(AuthorityIndex::new_for_test)
    } else if input.len() == 1 && input.chars().next()?.is_ascii_uppercase() {
        // Handle single uppercase ASCII alphabetic character
        let alpha_char = input.chars().next().unwrap();
        let index = alpha_char as u32 - 'A' as u32;
        Some(AuthorityIndex::new_for_test(index))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::block::BlockAPI;

    #[tokio::test]
    async fn test_dag_parsing() {
        telemetry_subscribers::init_for_testing();
        let dag_str = "DAG { 
            Round 0 : { 4 },
            Round 1 : { * },
            Round 2 : { * },
            Round 3 : {
                A -> [*],
                B -> [*],
                C -> [*],
                D -> [*],
            },
            Round 4 : {
                A -> [A3, B3, C3],
                B -> [A3, B3, C3],
                C -> [A3, B3, C3],
                D -> [*],
            },
            Round 5 : {
                A -> [*],
                B -> [-A4],
                C -> [-A4],
                D -> [-A4],
            },
            Round 6 : {
                A -> [A3, B3, C3, A1, B1],
                B -> [*, A0],
                C -> [-A5],
            }
         }";
        let result = parse_dag(dag_str);
        assert!(result.is_ok());

        let (_, dag_builder) = result.unwrap();
        assert_eq!(dag_builder.genesis.len(), 4);
        assert_eq!(dag_builder.blocks.len(), 23);

        // Check the blocks were correctly parsed in Round 6
        let blocks_a6 = dag_builder
            .get_uncommitted_blocks_at_slot(Slot::new(6, AuthorityIndex::new_for_test(0)));
        assert_eq!(blocks_a6.len(), 1);
        let block_a6 = blocks_a6.first().unwrap();
        assert_eq!(block_a6.round(), 6);
        assert_eq!(block_a6.author(), AuthorityIndex::new_for_test(0));
        assert_eq!(block_a6.ancestors().len(), 5);
        let expected_block_a6_ancestor_slots = [
            Slot::new(3, AuthorityIndex::new_for_test(0)),
            Slot::new(3, AuthorityIndex::new_for_test(1)),
            Slot::new(3, AuthorityIndex::new_for_test(2)),
            Slot::new(1, AuthorityIndex::new_for_test(0)),
            Slot::new(1, AuthorityIndex::new_for_test(1)),
        ];
        for ancestor in block_a6.ancestors() {
            assert!(expected_block_a6_ancestor_slots.contains(&Slot::from(*ancestor)));
        }

        let blocks_b6 = dag_builder
            .get_uncommitted_blocks_at_slot(Slot::new(6, AuthorityIndex::new_for_test(1)));
        assert_eq!(blocks_b6.len(), 1);
        let block_b6 = blocks_b6.first().unwrap();
        assert_eq!(block_b6.round(), 6);
        assert_eq!(block_b6.author(), AuthorityIndex::new_for_test(1));
        assert_eq!(block_b6.ancestors().len(), 5);
        let expected_block_b6_ancestor_slots = [
            Slot::new(5, AuthorityIndex::new_for_test(0)),
            Slot::new(5, AuthorityIndex::new_for_test(1)),
            Slot::new(5, AuthorityIndex::new_for_test(2)),
            Slot::new(5, AuthorityIndex::new_for_test(3)),
            Slot::new(0, AuthorityIndex::new_for_test(0)),
        ];
        for ancestor in block_b6.ancestors() {
            assert!(expected_block_b6_ancestor_slots.contains(&Slot::from(*ancestor)));
        }

        let blocks_c6 = dag_builder
            .get_uncommitted_blocks_at_slot(Slot::new(6, AuthorityIndex::new_for_test(2)));
        assert_eq!(blocks_c6.len(), 1);
        let block_c6 = blocks_c6.first().unwrap();
        assert_eq!(block_c6.round(), 6);
        assert_eq!(block_c6.author(), AuthorityIndex::new_for_test(2));
        assert_eq!(block_c6.ancestors().len(), 3);
        let expected_block_c6_ancestor_slots = [
            Slot::new(5, AuthorityIndex::new_for_test(1)),
            Slot::new(5, AuthorityIndex::new_for_test(2)),
            Slot::new(5, AuthorityIndex::new_for_test(3)),
        ];
        for ancestor in block_c6.ancestors() {
            assert!(expected_block_c6_ancestor_slots.contains(&Slot::from(*ancestor)));
        }
    }

    #[tokio::test]
    async fn test_genesis_round_parsing() {
        let dag_str = "Round 0 : { 4 }";
        let result = parse_genesis(dag_str);
        assert!(result.is_ok());
        let (_, num_authorities) = result.unwrap();

        assert_eq!(num_authorities, 4);
    }

    #[tokio::test]
    async fn test_slot_parsing() {
        let dag_str = "A0";
        let result = parse_slot(dag_str);
        assert!(result.is_ok());
        let (_, slot) = result.unwrap();

        assert_eq!(slot.authority, str_to_authority_index("A").unwrap());
        assert_eq!(slot.round, 0);
    }

    #[tokio::test]
    async fn test_all_round_parsing() {
        let dag_str = "Round 1 : { * }";
        let context = Arc::new(Context::new_for_test(4).0);
        let dag_builder = DagBuilder::new(context);
        let result = parse_round(dag_str, &dag_builder);
        assert!(result.is_ok());
        let (_, (round, connections)) = result.unwrap();

        assert_eq!(round, 1);
        for (i, (authority, references)) in connections.into_iter().enumerate() {
            assert_eq!(authority, AuthorityIndex::new_for_test(i as u32));
            assert_eq!(references, dag_builder.last_ancestors);
        }
    }

    #[tokio::test]
    async fn test_specific_round_parsing() {
        let dag_str = "Round 1 : {
            A -> [A0, B0, C0, D0],
            B -> [*, A0],
            C -> [-A0],
        }";
        let context = Arc::new(Context::new_for_test(4).0);
        let dag_builder = DagBuilder::new(context);
        let result = parse_round(dag_str, &dag_builder);
        assert!(result.is_ok());
        let (_, (round, connections)) = result.unwrap();

        let skipped_slot = Slot::new_for_test(0, 0); // A0
        let mut expected_references = [
            dag_builder.last_ancestors.clone(),
            dag_builder.last_ancestors.clone(),
            dag_builder
                .last_ancestors
                .into_iter()
                .filter(|ancestor| Slot::from(*ancestor) != skipped_slot)
                .collect(),
        ];

        assert_eq!(round, 1);
        for (i, (authority, mut references)) in connections.into_iter().enumerate() {
            assert_eq!(authority, AuthorityIndex::new_for_test(i as u32));
            references.sort();
            expected_references[i].sort();
            assert_eq!(references, expected_references[i]);
        }
    }

    #[tokio::test]
    async fn test_parse_author_and_connections() {
        let expected_authority = str_to_authority_index("A").unwrap();

        // case 1: all authorities
        let dag_str = "A -> [*]";
        let result = parse_author_and_connections(dag_str);
        assert!(result.is_ok());
        let (_, (actual_author, actual_connections)) = result.unwrap();
        assert_eq!(actual_author, expected_authority);
        assert_eq!(actual_connections, ["*"]);

        // case 2: specific included authorities
        let dag_str = "A -> [A0, B0, C0]";
        let result = parse_author_and_connections(dag_str);
        assert!(result.is_ok());
        let (_, (actual_author, actual_connections)) = result.unwrap();
        assert_eq!(actual_author, expected_authority);
        assert_eq!(actual_connections, ["A0", "B0", "C0"]);

        // case 3: specific excluded authorities
        let dag_str = "A -> [-A0, -B0]";
        let result = parse_author_and_connections(dag_str);
        assert!(result.is_ok());
        let (_, (actual_author, actual_connections)) = result.unwrap();
        assert_eq!(actual_author, expected_authority);
        assert_eq!(actual_connections, ["-A0", "-B0"]);

        // case 4: mixed all authorities + specific included/excluded authorities
        let dag_str = "A -> [*, A0, -B0]";
        let result = parse_author_and_connections(dag_str);
        assert!(result.is_ok());
        let (_, (actual_author, actual_connections)) = result.unwrap();
        assert_eq!(actual_author, expected_authority);
        assert_eq!(actual_connections, ["*", "A0", "-B0"]);

        // TODO: case 5: byzantine case of multiple blocks per slot; [*]; timestamp=1
    }

    #[tokio::test]
    async fn test_str_to_authority_index() {
        assert_eq!(
            str_to_authority_index("A"),
            Some(AuthorityIndex::new_for_test(0))
        );
        assert_eq!(
            str_to_authority_index("Z"),
            Some(AuthorityIndex::new_for_test(25))
        );
        assert_eq!(
            str_to_authority_index("[26]"),
            Some(AuthorityIndex::new_for_test(26))
        );
        assert_eq!(
            str_to_authority_index("[100]"),
            Some(AuthorityIndex::new_for_test(100))
        );
        assert_eq!(str_to_authority_index("a"), None);
        assert_eq!(str_to_authority_index("0"), None);
        assert_eq!(str_to_authority_index(" "), None);
        assert_eq!(str_to_authority_index("!"), None);
    }
}
