// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import dotenv from 'dotenv';
import path from 'path';
import { pino } from 'pino';

export enum Level {
  fatal = 'fatal',
  error = 'error',
  warn = 'warn',
  info = 'info',
  debug = 'debug',
  trace = 'trace',
  silent = 'silent', // use this to disable logging
}

/**
 * Singleton class to handle logging
 */
class Logger {
  private static levelsTranslator: { [key: number]: string } = {
    10: 'trace',
    20: 'debug',
    30: 'info',
    40: 'warn',
    50: 'error',
    60: 'fatal',
  };

  private static instance: Logger;
  private logger: pino.Logger;
  private constructor(level: Level) {
    const pinoLogger = pino(
      {
        base: null,
        level,
        timestamp: () => `,"time":"${new Date().toISOString()}"`,
        pool_id: (id: string) => id,
        formatters: {
          level(label, number) {
            return { level: Logger.levelsTranslator[number] };
          },
          log(object) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            const { ...rest } = object;
            return rest;
          },
        },
        depthLimit: 10,
      },
      process.stdout,
    );
    this.logger = pinoLogger;
    Logger.instance = this;
  }

  /**
   * Initialize the logger instance.
   * @param level {Level} - The minimum level to log.
   */
  public static initialize(level: Level = Level.error) {
    if (!Logger.instance) {
      new Logger(level);
      return Logger.instance;
    }
    return Logger.instance;
  }

  /**
   * Wrapper method for the logging-library's log function.
   * @param level {Level} - The level to log at.
   * @param msg {string} - The message regarding the log.
   * @param pool_id {string} - The pool id to log; Used for tracing.
   */
  public log(level: Level = Level.info, msg: string, pool_id?: string): void {
    Logger.instance.logger[level]({ msg, pool_id });
  }
}

dotenv.config({
  path: path.resolve(__dirname, '../test/.test.env'),
});
export const logger = Logger.initialize(process.env.LOGGING_LEVEL as Level);
