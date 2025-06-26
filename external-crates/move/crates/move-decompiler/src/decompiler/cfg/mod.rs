// Copyright (c) Verichains, 2023

pub mod algo;

pub type StacklessBlockIdentifier = usize;
pub type StacklessBlockContent = algo::blocks_stackless::StacklessBlockContent;

pub mod datastructs;
pub mod stackless;
pub mod metadata;
