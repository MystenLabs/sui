// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as vscode from 'vscode';
import * as lc from 'vscode-languageclient/node';

type ServerState = 'starting' | 'idle' | 'busy' | 'slow' | 'stopped';

// How often the dot animation advances (. → .. → ...)
const ANIMATION_INTERVAL_MS = 500;
// How often pending requests are checked for exceeding the slow threshold
const SLOW_CHECK_INTERVAL_MS = 2000;
// Time after which a busy/starting state is promoted to slow (yellow)
const SLOW_THRESHOLD_MS = 10000;
const DOT_FRAMES = ['.', '..', '...'];
const BASE_LABEL = 'move-analyzer';

/**
 * Activity monitor: displays server health in the VS Code status bar.
 * Driven by a state machine:
 *
 *   starting ──► idle ◄──► busy ──► slow
 *                  ▲                  │
 *                  └──────────────────┘
 *   stopped (terminal until client restarts)
 *
 * Starting/idle/stopped come from LanguageClient.onDidChangeState
 * (maps Starting→starting, Running→idle, Stopped→stopped).
 *
 * Busy/slow detection uses two complementary signals:
 * - Server-sent $/progress notifications (compilation start/end)
 * - Client-side sendRequest wrapper (individual request latency)
 * Either source can trigger busy; idle requires both to be quiet.
 *
 * Slow promotion: on entering starting or busy, a one-shot timer
 * (compilationSlowTimer) is scheduled at SLOW_THRESHOLD_MS. If the
 * state hasn't changed by the time it fires, it promotes to slow
 * (yellow). Any state transition calls stopTimers(), cancelling the
 * pending timer — so a quick busy→idle round-trip never turns yellow.
 * A separate interval (slowCheckTimer) periodically scans pending
 * requests for individually slow responses.
 */
export class ServerActivityMonitor implements vscode.Disposable {
    private readonly item: vscode.StatusBarItem;

    private readonly extensionVersion: string;

    private readonly serverVersion: string;

    private state: ServerState = 'starting';

    // Tracks whether server is currently compiling (from $/progress)
    private compilationInProgress = false;

    // Maps tracking IDs → request-sent timestamps for slow request detection
    private readonly pendingRequests: Map<string, number> = new Map();

    private animationTimer: ReturnType<typeof setInterval> | undefined;

    private slowCheckTimer: ReturnType<typeof setInterval> | undefined;

    // Fires once after SLOW_THRESHOLD_MS to promote starting/busy → slow
    private compilationSlowTimer: ReturnType<typeof setTimeout> | undefined;

    private animationStep = 0;

    constructor(extensionVersion: string, serverVersion: string, command?: string) {
        this.extensionVersion = extensionVersion;
        this.serverVersion = serverVersion;
        this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
        this.item.name = 'Move Analyzer';
        if (command !== undefined && command.length > 0) {
            this.item.command = command;
        }
        this.render();
        this.item.show();
    }

    // Called from client.onDidChangeState; resets all tracking on transitions
    // since a stopped/restarted server won't respond to old requests.
    onClientStateChange(_oldState: lc.State, newState: lc.State): void {
        if (newState === lc.State.Running) {
            this.pendingRequests.clear();
            this.compilationInProgress = false;
            this.transitionTo('idle');
        } else if (newState === lc.State.Stopped) {
            this.pendingRequests.clear();
            this.compilationInProgress = false;
            this.transitionTo('stopped');
        } else {
            this.transitionTo('starting');
        }
    }

    onCompilationStart(): void {
        this.compilationInProgress = true;
        if (this.state === 'idle') {
            this.transitionTo('busy');
        }
    }

    onCompilationEnd(): void {
        this.compilationInProgress = false;
        if (this.pendingRequests.size === 0 && (this.state === 'busy' || this.state === 'slow')) {
            this.transitionTo('idle');
        }
    }

    onRequestSent(trackingId: string): void {
        this.pendingRequests.set(trackingId, Date.now());
        if (this.state === 'idle') {
            this.transitionTo('busy');
        }
    }

    onResponseReceived(trackingId: string): void {
        this.pendingRequests.delete(trackingId);
        if (!this.compilationInProgress
            && this.pendingRequests.size === 0
            && (this.state === 'busy' || this.state === 'slow')) {
            this.transitionTo('idle');
        }
    }

    dispose(): void {
        this.stopTimers();
        this.item.dispose();
    }

    private transitionTo(newState: ServerState): void {
        this.state = newState;
        this.stopTimers();

        const needsAnimation = newState === 'starting' || newState === 'busy' || newState === 'slow';
        if (needsAnimation) {
            this.animationStep = 0;
            this.animationTimer = setInterval(() => {
                this.animationStep = (this.animationStep + 1) % DOT_FRAMES.length;
                this.render();
            }, ANIMATION_INTERVAL_MS);
        }

        // Promote starting/busy → slow after SLOW_THRESHOLD_MS.
        // For 'starting': the LSP handshake is taking too long.
        // For 'busy': compilation or an individual request is taking too long.
        if (newState === 'starting' || newState === 'busy') {
            this.compilationSlowTimer = setTimeout(() => {
                if (this.state === 'starting' || this.state === 'busy') {
                    this.transitionTo('slow');
                }
            }, SLOW_THRESHOLD_MS);
        }

        if (newState === 'busy') {
            // Also check for slow individual requests periodically
            this.slowCheckTimer = setInterval(() => {
                const now = Date.now();
                for (const timestamp of this.pendingRequests.values()) {
                    if (now - timestamp > SLOW_THRESHOLD_MS) {
                        this.transitionTo('slow');
                        return;
                    }
                }
            }, SLOW_CHECK_INTERVAL_MS);
        }

        this.render();
    }

    private stopTimers(): void {
        if (this.animationTimer !== undefined) {
            clearInterval(this.animationTimer);
            this.animationTimer = undefined;
        }
        if (this.slowCheckTimer !== undefined) {
            clearInterval(this.slowCheckTimer);
            this.slowCheckTimer = undefined;
        }
        if (this.compilationSlowTimer !== undefined) {
            clearTimeout(this.compilationSlowTimer);
            this.compilationSlowTimer = undefined;
        }
    }

    private render(): void {
        const needsAnimation = this.state === 'starting' || this.state === 'busy' || this.state === 'slow';

        if (this.state === 'stopped') {
            this.item.text = `$(error) ${BASE_LABEL}`;
        } else if (needsAnimation) {
            this.item.text = `${BASE_LABEL} ${DOT_FRAMES[this.animationStep]}`;
        } else {
            this.item.text = BASE_LABEL;
        }

        if (this.state === 'stopped') {
            this.item.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
        } else if (this.state === 'slow') {
            this.item.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
        } else {
            this.item.backgroundColor = undefined;
        }

        const statusText = this.state === 'stopped' ? 'Stopped' : 'Running';
        const tooltip = new vscode.MarkdownString(undefined, true);
        // Enables command: URIs so the restart link works
        tooltip.isTrusted = true;
        tooltip.appendMarkdown('**Move Analyzer**\n\n');
        tooltip.appendMarkdown(`Extension: v${this.extensionVersion}\n\n`);
        tooltip.appendMarkdown(`Server: ${this.serverVersion}\n\n`);
        tooltip.appendMarkdown(`Status: ${statusText}\n\n`);
        tooltip.appendMarkdown('---\n\n');
        tooltip.appendMarkdown('[Restart Server](command:move.serverRestart)');
        this.item.tooltip = tooltip;
    }
}
