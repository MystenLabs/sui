// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { graphql } from '@mysten/sui.js/graphql/schemas/2024-01';
import { Pagination, benchmark_connection_query, metrics } from './benchmark';
import { SuiGraphQLClient } from '@mysten/sui.js/graphql';


export const SingleCheckpoint = graphql(
    `query SingleCheckpoint($digest: String, $seqNum: Int) {
        checkpoint(id: {digest: $digest, sequenceNumber: $seqNum}) {
          sequenceNumber
        }
      }`
);

export const EpochCheckpoints = graphql(
    `query EpochCheckpoints($epochId: Int, $first: Int, $after: String, $last: Int, $before: String) {
        epoch(id: $epochId) {
          checkpoints(first: $first, after: $after, last: $last, before: $before) {
            pageInfo {
                startCursor
                endCursor
                hasNextPage
                hasPreviousPage
            }
            nodes {
              sequenceNumber
            }
          }
        }
      }`
);

export const Checkpoints = graphql(
    `query Checkpoints($first: Int, $after: String, $last: Int, $before: String) {
        checkpoints(first: $first, after: $after, last: $last, before: $before) {
          pageInfo {
            startCursor
            endCursor
            hasNextPage
            hasPreviousPage
          }
          nodes {
            sequenceNumber
          }
        }
      }`
);

// TODO: can we share function params?
// TODO: how can we combine queries together? For example, if I want to run 50 `SingleCheckpoint` in a single graphql request

export const queries = {
    EpochCheckpoints,
    Checkpoints
};

const client = new SuiGraphQLClient({
	url: 'http://127.0.0.1:8000',
	queries
});

async function checkpoints(client: SuiGraphQLClient<typeof queries>, pagination: Pagination) {
  let { paginateForwards, limit, numPages } = pagination;
  console.log('Checkpoints, paginateForwards: ' + paginateForwards + ', limit: ' + limit + ', numPages: ' + numPages);

    let durations = await benchmark_connection_query(client, paginateForwards, async (client, cursor) => {
        let variables = paginateForwards ? { first: limit, after: cursor } : { last: limit, before: cursor };

        const response = await client.execute('Checkpoints', {
            variables
        });
        const data = response.data;
        const pageInfo = data?.checkpoints.pageInfo;
        return pageInfo;
    }, numPages).catch ((error) => {
        console.error(error);
        return [];
    });

    console.log(metrics(durations));
}

async function epochCheckpoints(client: SuiGraphQLClient<typeof queries>, pagination: Pagination, epochId: number | null) {
  let { paginateForwards, limit, numPages } = pagination;
  console.log('EpochCheckpoints, paginateForwards: ' + paginateForwards + ', limit: ' + limit + ', numPages: ' + numPages + ', epochId: ' + epochId);

    let durations = await benchmark_connection_query(client, paginateForwards, async (client, cursor) => {
        let variables = paginateForwards ? { first: limit, after: cursor } : { last: limit, before: cursor };
        const response = await client.execute('EpochCheckpoints', {
            variables: {
                ...variables,
                epochId
            }
        });
        const data = response.data;
        const pageInfo = data?.epoch!.checkpoints.pageInfo;
        return pageInfo;
    }, numPages).catch ((error) => {
        console.error(error);
        return [];
    });

    console.log(metrics(durations));
}

async function checkpointSuite(client: SuiGraphQLClient<typeof queries>) {
  let paginateForwards = true;

  let pagination = {
      paginateForwards,
      limit: 50,
      numPages: 10
  };

    await checkpoints(client, pagination);
    await checkpoints(client, { ...pagination, paginateForwards: false });

    let epochId = 320;
    await epochCheckpoints(client, pagination, epochId);
    await epochCheckpoints(client, { ...pagination, paginateForwards: false }, epochId);
};

checkpointSuite(client);
