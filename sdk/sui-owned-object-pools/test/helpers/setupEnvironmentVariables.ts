// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import * as dotenv from 'dotenv';
import * as path from 'path';

/**
 * Represents the environment variables used in the unit tests of the library.
 */
export type EnvironmentVariables = {
  NFT_APP_PACKAGE_ID: string;
  NFT_APP_ADMIN_CAP: string;
  SUI_NODE: string;
  ADMIN_ADDRESS: string;
  ADMIN_SECRET_KEY: string;
  TEST_USER_ADDRESS: string;
  TEST_USER_SECRET: string;
  GET_WORKER_TIMEOUT_MS: number;
};

/**
 * Retrieves environment variables from a specified path and returns them as an object.
 * @param pathToEnv - The path to the environment file. Defaults to '../.env'.
 * @param isTest - A boolean indicating whether the function is being called in a test environment.
 * Useful for checking if all the required environment variables are present.
 * The required environment variables between test and non-test environments could differ.
 * @returns An object containing the retrieved environment variables.
 */
export function getEnvironmentVariables(pathToEnv = '../.env', isTest = false) {
  dotenv.config({
    path: path.resolve(__dirname, pathToEnv),
  });

  const env = {
    NFT_APP_PACKAGE_ID: process.env.NFT_APP_PACKAGE_ID ?? '',
    NFT_APP_ADMIN_CAP: process.env.NFT_APP_ADMIN_CAP ?? '',
    SUI_NODE: process.env.SUI_NODE ?? '',
    ADMIN_ADDRESS: process.env.ADMIN_ADDRESS ?? '',
    ADMIN_SECRET_KEY: process.env.ADMIN_SECRET_KEY ?? '',
    TEST_USER_ADDRESS: process.env.TEST_USER_ADDRESS ?? '',
    TEST_USER_SECRET: process.env.TEST_USER_SECRET ?? '',
    GET_WORKER_TIMEOUT_MS: parseInt(
      process.env.GET_WORKER_TIMEOUT_MS ?? '10000',
    ),
  } as EnvironmentVariables;

  if (isTest) {
    const testEnvVariables: string[] = Array.from(Object.keys(env));
    checkForMissingEnvVariables(env, testEnvVariables);
  }
  return env;
}

/**
 * Checks if the required environment variables are present and have a value.
 * Throws an error if any of the required environment variables are missing.
 *
 * @param env - An object containing the environment variables to check.
 * @param envVariablesToCheck - An array of strings representing the names of the environment variables to check.
 * @throws {Error} If any of the required environment variables are missing.
 */
function checkForMissingEnvVariables(
  env: EnvironmentVariables,
  envVariablesToCheck: string[],
) {
  for (const [key, value] of Object.entries(env)) {
    if (envVariablesToCheck.includes(key) && !value) {
      throw new Error(`Missing environment variable ${key}`);
    }
  }
}
