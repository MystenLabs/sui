import {
    Breakpoint,
    Handles,
    Logger,
    logger,
    LoggingDebugSession,
    InitializedEvent,
    TerminatedEvent,
    StoppedEvent,
    Thread,
    Scope,
    StackFrame,
    Source
} from '@vscode/debugadapter';
import { DebugProtocol } from '@vscode/debugprotocol';
import * as path from 'path';
import {
    Runtime,
    RuntimeEvents,
    RuntimeValueType,
    IRuntimeVariableScope,
    CompoundType,
    IRuntimeRefValue,
    ExecutionResult,
    IMoveCallStack
} from './runtime';
import { PTB_START_FRAME_ID } from './trace_utils';


const SUMMARY_FRAME_SRC_REF = 42;

const enum LogLevel {
    Log = 'log',
    Verbose = 'verbose',
    None = 'none'
}

/**
 * Customized stack trace response that includes additional data.
 */
interface CustomizedStackTraceResponse extends DebugProtocol.StackTraceResponse {
    body: {
        stackFrames: StackFrame[];
        totalFrames?: number;
        optimizedLines: number[];
    };
}

/**
 * Converts a log level string to a Logger.LogLevel.
 *
 * @param level log level as string
 * @returns log level as Logger.LogLevel
 */
function convertLoggerLogLevel(level: string): Logger.LogLevel {
    switch (level) {
        case LogLevel.Log:
            return Logger.LogLevel.Log;
        case LogLevel.Verbose:
            return Logger.LogLevel.Verbose;
        default:
            return Logger.LogLevel.Stop;
    }
}

/**
 * This interface describes the move-debug specific launch attributes
 * (which are not part of the Debug Adapter Protocol).
 * The schema for these attributes lives in the package.json of the move-debug extension.
 * The interface should always match this schema.
 */
interface ILaunchRequestArguments extends DebugProtocol.LaunchRequestArguments {
    /** An absolute path to the Move source file whose traces are to be viewed. */
    source: string;
    /** Trace selected for viewing. */
    traceInfo: string;
    /** Automatically stop target after launch. If not specified, target does not stop. */
    stopOnEntry?: boolean;
    /** enable logging the Debug Adapter Protocol */
    logLevel?: string;
}

export class MoveDebugSession extends LoggingDebugSession {

    /**
     * We only even have one thread so we can hardcode its ID.
     */
    private static THREAD_ID = 1;

    /**
     * DAP-independent runtime maintaining state during
     * trace viewing session.
     */
    private runtime: Runtime;

    /**
     * Handles to create variable scopes and compound variable values.
     */
    private variableHandles: Handles<IRuntimeVariableScope | CompoundType>;

    private count: number = 0;

    public constructor() {
        super();
        this.setDebuggerLinesStartAt1(false);
        this.setDebuggerColumnsStartAt1(false);
        this.runtime = new Runtime();
        this.variableHandles = new Handles<IRuntimeVariableScope | CompoundType>();

        // setup event handlers

        this.runtime.on(RuntimeEvents.stopOnStep, () => {
            this.sendEvent(new StoppedEvent('step', MoveDebugSession.THREAD_ID));
        });
        this.runtime.on(RuntimeEvents.stopOnLineBreakpoint, () => {
            this.sendEvent(new StoppedEvent('breakpoint', MoveDebugSession.THREAD_ID));
        });
        this.runtime.on(RuntimeEvents.stopOnException, (msg) => {
            this.sendEvent(new StoppedEvent('exception', MoveDebugSession.THREAD_ID, msg));
        });
        this.runtime.on(RuntimeEvents.end, () => {
            this.sendEvent(new TerminatedEvent());
        });

    }

    protected initializeRequest(response: DebugProtocol.InitializeResponse, args: DebugProtocol.InitializeRequestArguments): void {

        // build and return the capabilities of this debug adapter (enable as needed)
        response.body = response.body || {};

        // the adapter implements the configurationDone request
        response.body.supportsConfigurationDoneRequest = false;

        // the adapter supports conditional breakpoints
        response.body.supportsConditionalBreakpoints = false;

        // the adapter supports breakpoints that break execution after a specified number of hits
        response.body.supportsHitConditionalBreakpoints = false;

        // make VS Code use 'evaluate' when hovering over source
        response.body.supportsEvaluateForHovers = false;

        // make VS Code show a 'step back' button
        response.body.supportsStepBack = false;

        // make VS Code support data breakpoints
        response.body.supportsDataBreakpoints = false;

        // make VS Code support completion in REPL
        response.body.supportsCompletionsRequest = false;
        response.body.completionTriggerCharacters = [];

        // make VS Code send cancel request
        response.body.supportsCancelRequest = false;

        // make VS Code send the breakpointLocations request
        response.body.supportsBreakpointLocationsRequest = false;

        // make VS Code provide "Step in Target" functionality
        response.body.supportsStepInTargetsRequest = false;

        // the adapter defines two exceptions filters, one with support for conditions.
        response.body.supportsExceptionFilterOptions = false;
        response.body.exceptionBreakpointFilters = [];

        // make VS Code send exceptionInfo request
        response.body.supportsExceptionInfoRequest = false;

        // make VS Code send setVariable request
        response.body.supportsSetVariable = false;

        // make VS Code send setExpression request
        response.body.supportsSetExpression = false;

        // make VS Code send disassemble request (it's false
        // as we handle this differently through custom commands)
        response.body.supportsDisassembleRequest = false;
        response.body.supportsSteppingGranularity = false;
        response.body.supportsInstructionBreakpoints = false;

        // make VS Code able to read and write variable memory
        response.body.supportsReadMemoryRequest = false;
        response.body.supportsWriteMemoryRequest = false;

        response.body.supportSuspendDebuggee = false;
        response.body.supportTerminateDebuggee = false;
        response.body.supportsFunctionBreakpoints = false;
        response.body.supportsDelayedStackTraceLoading = false;

        this.sendResponse(response);
        this.sendEvent(new InitializedEvent());
    }

    /**
     * Intercepts all requests sent to the debug adapter to handle custom ones.
     *
     * @param request request to be dispatched.
     */
    protected dispatchRequest(request: DebugProtocol.Request): void {
        if (request.command === 'toggleDisassembly') {
            this.runtime.toggleDisassembly();
            this.sendEvent(new StoppedEvent('toggle disassembly', MoveDebugSession.THREAD_ID));
        } else if (request.command === 'toggleSource') {
            this.runtime.toggleSource();
            this.sendEvent(new StoppedEvent('toggle source', MoveDebugSession.THREAD_ID));
        } else if (request.command === 'fileChanged') {
            const newFilePath = String(request.arguments);
            const changedFilePath = this.runtime.setCurrentMoveFileFromPath(newFilePath);
            logger.log('Current Move file changed to ' + changedFilePath);
        } else {
            super.dispatchRequest(request);
        }
    }

    protected async launchRequest(
        response: DebugProtocol.LaunchResponse,
        args: ILaunchRequestArguments
    ): Promise<void> {
        logger.setup(convertLoggerLogLevel(args.logLevel ?? LogLevel.None), false);
        logger.log(`Launching trace viewer for file: ${args.source} and trace: ${args.traceInfo}`);

        try {
            await this.runtime.start(args.source, args.traceInfo, args.stopOnEntry || false);
        } catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
        this.sendEvent(new StoppedEvent('entry', MoveDebugSession.THREAD_ID));
    }

    protected threadsRequest(response: DebugProtocol.ThreadsResponse): void {
        response.body = {
            threads: [
                new Thread(MoveDebugSession.THREAD_ID, 'Main Thread')
            ]
        };
        this.sendResponse(response);
    }

    protected stackTraceRequest(
        response: CustomizedStackTraceResponse,
        _args: DebugProtocol.StackTraceArguments
    ): void {
        try {
            const stackFrames = [];
            let optimizedLines: number[] = [];
            const ptbStack = this.runtime.stack();
            if (ptbStack.summaryFrame) {
                const name = 'PTB Summary';
                let summaryFrameSrc = new Source(name);
                summaryFrameSrc.sourceReference = SUMMARY_FRAME_SRC_REF;
                const summaryFrame = new StackFrame(
                    ptbStack.summaryFrame.id,
                    name,
                    summaryFrameSrc,
                    ptbStack.summaryFrame.line
                );
                stackFrames.push(summaryFrame);
            }
            const cmdFrame = ptbStack.commandFrame;
            if (cmdFrame) {
                if ('frames' in cmdFrame && 'globals' in cmdFrame) {
                    const moveCallStack = cmdFrame as IMoveCallStack;
                    const stack_height = moveCallStack.frames.length;
                    stackFrames.push(...moveCallStack.frames.map(frame => {
                        const fileName = frame.disassemblyModeTriggered
                            ? path.basename(frame.bcodeFilePath!)
                            : path.basename(frame.srcFilePath);
                        const frameSource = frame.disassemblyModeTriggered
                            ? new Source(fileName, frame.bcodeFilePath!)
                            : new Source(fileName, frame.srcFilePath);
                        const currentLine = frame.disassemblyModeTriggered
                            ? frame.bcodeLine!
                            : frame.srcLine;
                        return new StackFrame(frame.id, frame.name, frameSource, currentLine);
                    }));
                    if (stack_height > 0) {
                        optimizedLines = moveCallStack.frames[stack_height - 1].disassemblyModeTriggered
                            ? moveCallStack.frames[stack_height - 1].optimizedBcodeLines!
                            : moveCallStack.frames[stack_height - 1].optimizedSrcLines;
                    }
                } else {
                    // TODO: create and push a frame for native PTB command
                }
            }
            response.body = {
                stackFrames: stackFrames.reverse(),
                optimizedLines
            };
        } catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
    }

    protected sourceRequest(
        response: DebugProtocol.SourceResponse,
        args: DebugProtocol.SourceArguments
    ): void {
        let content = '';
        if (args.sourceReference === SUMMARY_FRAME_SRC_REF) {
            const summaryFrame = this.runtime.stack().summaryFrame;
            if (summaryFrame) {
                for (const summary of summaryFrame.summary) {
                    const summaryStr = typeof summary === 'string'
                        ? summary
                        : summary.pkg + '::' + summary.module + '::' + summary.function;
                    content += summaryStr + '\n';
                }

            } else {
                content = 'No PTB summary available';
            };
        } else {
            content = 'Unknown source';
        }
        response.body = {
            content,
            mimeType: 'text/plain',
        };
        this.sendResponse(response);
    }

    /**
     * Gets the scopes for a given frame.
     *
     * @param frameID identifier of the frame scopes are requested for.
     * @returns an array of scopes.
     * @throws Error with a descriptive error message if scopes cannot be retrieved.
     */
    private getScopes(frameID: number): DebugProtocol.Scope[] {
        const scopes: DebugProtocol.Scope[] = [];
        if (frameID === PTB_START_FRAME_ID) {
            // no scopes for the summary frame
            return scopes;
        }
        const ptbStack = this.runtime.stack();
        const cmdFrame = ptbStack.commandFrame;
        if (!cmdFrame) {
            return scopes;
        }
        if ('frames' in cmdFrame && 'globals' in cmdFrame) {
            const frame = cmdFrame.frames.find(frame => frame.id === frameID);
            if (!frame) {
                throw new Error(`No frame found for id: ${frameID} when getting scopes`);
            }
            if (frame.locals.length > 0) {
                for (let i = frame.locals.length - 1; i > 0; i--) {
                    const shadowedScopeReference = this.variableHandles.create({ locals: frame.locals[i] });
                    const shadowedScope = new Scope(`shadowed(${i}): ${frame.name}`, shadowedScopeReference, false);
                    scopes.push(shadowedScope);
                }
            }
            // don't have to check if scope 0 exists as it's created whenever a new frame is created
            // and it's never disposed of
            const localScopeReference = this.variableHandles.create({ locals: frame.locals[0] });
            const localScope = new Scope(`locals: ${frame.name}`, localScopeReference, false);
            scopes.push(localScope);
        } else {
            // TODO: return scopes for a native  PTB command
        }
        return scopes;
    }

    protected scopesRequest(
        response: DebugProtocol.ScopesResponse,
        args: DebugProtocol.ScopesArguments
    ): void {
        try {
            const scopes = this.getScopes(args.frameId);
            const changedFile = this.runtime.setCurrentMoveFileFromFrame(args.frameId);
            logger.log('Current Move file changed to '
                + changedFile
                + ' for frame id '
                + args.frameId);
            response.body = {
                scopes
            };
        } catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
    }

    /**
     * Converts a Move reference value to a DAP variable.
     *
     * @param moveCallStack the current Move call stack
     * @param value reference value.
     * @param name name of variable containing the reference value.
     * @param type optional type of the variable containing the reference value.
     * @returns a DAP variable.
     * @throws Error with a descriptive error message if conversion fails.
     */
    private convertMoveRefValue(
        moveCallStack: IMoveCallStack,
        value: IRuntimeRefValue,
        name: string,
        type?: string
    ): DebugProtocol.Variable {
        const indexedLoc = value.indexedLoc;
        if ('globalIndex' in indexedLoc.loc) {
            // global location
            const globalValue = moveCallStack.globals.get(indexedLoc.loc.globalIndex);
            if (!globalValue) {
                throw new Error('No global found for index '
                    + indexedLoc.loc.globalIndex
                    + ' when converting Move call ref value ');
            }
            const indexPath = [...indexedLoc.indexPath];
            return this.convertMoveValue(moveCallStack, globalValue, name, indexPath, type);
        } else if ('frameID' in indexedLoc.loc && 'localIndex' in indexedLoc.loc) {
            // local variable
            const frameID = indexedLoc.loc.frameID;
            const localIndex = indexedLoc.loc.localIndex;
            const frame = moveCallStack.frames.find(frame => frame.id === frameID);
            if (!frame) {
                throw new Error('No frame found for id '
                    + frameID
                    + ' when converting Move call ref value for local index '
                    + localIndex);
            }
            // a local will be in one of the scopes at a position corresponding to its local index
            let local = undefined;
            for (const scope of frame.locals) {
                local = scope[localIndex];
                if (local) {
                    break;
                }
            }
            if (!local) {
                throw new Error('No local found for index '
                    + localIndex
                    + ' when converting Move call ref value for frame id '
                    + frameID);
            }
            const indexPath = [...indexedLoc.indexPath];
            return this.convertMoveValue(moveCallStack, local.value, name, indexPath, type);
        } else {
            throw new Error('Invalid runtime location when comverting Move call ref value');
        }
    }

    /**
     * Converts a Move value to a DAP variable.
     *
     * @param moveCallStack the current Move call stack
     * @param value variable value
     * @param name variable name
     * @param indexPath a path to actual value for compound types (e.g, [1, 7] means
     * first field/vector element and then seventh field/vector element)
     * @param type optional variable type
     * @returns a DAP variable.
     * @throws Error with a descriptive error message if conversion has failed.
     */
    private convertMoveValue(
        moveCallStack: IMoveCallStack,
        value: RuntimeValueType,
        name: string,
        indexPath: number[],
        type?: string,
    ): DebugProtocol.Variable {
        if (typeof value === 'string') {
            if (indexPath.length > 0) {
                throw new Error('Cannot index into a string');
            }
            return {
                name,
                type,
                value,
                variablesReference: 0
            };
        } else if (Array.isArray(value)) {
            if (indexPath.length > 0) {
                const index = indexPath.pop();
                if (index === undefined || index >= value.length) {
                    throw new Error('Index path for an array is invalid');
                }
                return this.convertMoveValue(moveCallStack, value[index], name, indexPath, type);
            }
            const compoundValueReference = this.variableHandles.create(value);
            return {
                name,
                type,
                value: '(' + value.length + ')[...]',
                variablesReference: compoundValueReference
            };
        } else if ('fields' in value) {
            if (indexPath.length > 0) {
                const index = indexPath.pop();
                if (index === undefined || index >= value.fields.length) {
                    throw new Error('Index path for a compound type is invalid');
                }
                return this.convertMoveValue(moveCallStack, value.fields[index][1], name, indexPath, type);
            }
            const compoundValueReference = this.variableHandles.create(value);
            // use type if available as it will have information about whether
            // it's a reference or not (e.g., `&mut 0x42::mod::SomeStruct`),
            // as opposed to the type that come with the value
            // (e.g., `0x42::mod::SomeStruct`)
            const actualType = type ? type : value.type;
            const accessChainParts = actualType.split('::');
            const datatypeName = accessChainParts[accessChainParts.length - 1];
            return {
                name,
                type: value.variantName
                    ? actualType + '::' + value.variantName
                    : actualType,
                value: (value.variantName
                    ? datatypeName + '::' + value.variantName
                    : datatypeName
                ) + '{...}',
                variablesReference: compoundValueReference
            };
        } else {
            if (indexPath.length > 0) {
                throw new Error('Cannot index into a reference value');
            }
            return this.convertMoveRefValue(moveCallStack, value, name, type);
        }
    }

    /**
     * Converts runtime variables to DAP variables.
     *
     * @param runtimeScope runtime variables scope,
     * @returns an array of DAP variables.
     */
    private convertRuntimeVariables(
        runtimeScope: IRuntimeVariableScope,
        moveCallStack: IMoveCallStack
    ): DebugProtocol.Variable[] {
        const variables: DebugProtocol.Variable[] = [];
        const runtimeVariables = runtimeScope.locals;
        let disassemblyView = false;
        if (runtimeVariables.length > 0) {
            // there can be undefined entries in the variables array,
            // so find any non-undefined one (they will all point to
            // the same frame)
            const firstVar = runtimeVariables.find(v => v);
            if (firstVar) {
                const varFrame = moveCallStack.frames[firstVar.frameIdx];
                if (varFrame) {
                    disassemblyView = varFrame.disassemblyView;
                }
            }
        }
        runtimeVariables.forEach(v => {
            if (v) {
                const varName = disassemblyView
                    ? v.info.internalName
                    : v.info.name;
                const dapVar = this.convertMoveValue(moveCallStack, v.value, varName, [], v.type);
                if (disassemblyView || !varName.includes('%')) {
                    // Don't show "artificial" variables generated by the compiler
                    // for enum and macro execution when showing source code as they
                    // would be quite confusing for the user without knowing compilation
                    // internals. On the other hand, it make sense to show them when showing
                    // disassembly
                    variables.push(dapVar);
                }
            }
        });
        return variables;
    }

    protected variablesRequest(
        response: DebugProtocol.VariablesResponse,
        args: DebugProtocol.VariablesArguments
    ): void {
        try {
            let variables: DebugProtocol.Variable[] = [];
            const ptbStack = this.runtime.stack();
            const cmdFrame = ptbStack.commandFrame;
            if (ptbStack.summaryFrame && !cmdFrame) {
                // no variables for summary frame
                this.sendResponse(response);
            }
            if (cmdFrame) {
                if ('frames' in cmdFrame && 'globals' in cmdFrame) {
                    const moveCallStack = cmdFrame as IMoveCallStack;
                    const variableHandle = this.variableHandles.get(args.variablesReference);
                    if (variableHandle) {
                        if ('locals' in variableHandle) {
                            // we are dealing with a scope
                            variables = this.convertRuntimeVariables(variableHandle, moveCallStack);
                        } else {
                            // we are dealing with a compound value
                            if (Array.isArray(variableHandle)) {
                                for (let i = 0; i < variableHandle.length; i++) {
                                    const v = variableHandle[i];
                                    variables.push(this.convertMoveValue(moveCallStack, v, String(i), []));
                                }
                            } else {
                                variableHandle.fields.forEach(([fname, fvalue]) => {
                                    variables.push(this.convertMoveValue(moveCallStack, fvalue, fname, []));
                                });
                            }
                        }
                    }
                } else {
                    // TODO: handle variables for native PTB commands
                }
            }
            if (variables.length > 0) {
                response.body = {
                    variables
                };
            }
        } catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
    }


    protected nextRequest(
        response: DebugProtocol.NextResponse,
        _args: DebugProtocol.NextArguments
    ): void {
        let terminate = false;
        try {
            const executionResult = this.runtime.step(/* next */ true, /* stopAtCloseFrame */ false);
            terminate = executionResult === ExecutionResult.TraceEnd;
        } catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        if (terminate) {
            this.sendEvent(new TerminatedEvent());
        }
        this.sendResponse(response);
    }

    protected stepInRequest(
        response: DebugProtocol.StepInResponse,
        _args: DebugProtocol.StepInArguments
    ): void {
        let terminate = false;
        try {
            const executionResult = this.runtime.step(/* next */ false, /* stopAtCloseFrame */ false);
            terminate = executionResult === ExecutionResult.TraceEnd;
        } catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        if (terminate) {
            this.sendEvent(new TerminatedEvent());
        }
        this.sendResponse(response);
    }

    protected stepOutRequest(
        response: DebugProtocol.StepOutResponse,
        _args: DebugProtocol.StepOutArguments
    ): void {
        let terminate = false;
        try {
            const executionResult = this.runtime.stepOut(/* next */ false);
            terminate = executionResult === ExecutionResult.TraceEnd;
        } catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        if (terminate) {
            this.sendEvent(new TerminatedEvent());
        }
        this.sendResponse(response);
    }

    protected continueRequest(
        response: DebugProtocol.ContinueResponse,
        _args: DebugProtocol.ContinueArguments
    ): void {
        let terminate = false;
        try {
            const executionResult = this.runtime.continue();
            terminate = executionResult === ExecutionResult.TraceEnd;
        } catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        if (terminate) {
            this.sendEvent(new TerminatedEvent());
        }
        this.sendResponse(response);
    }

    protected setBreakPointsRequest(response: DebugProtocol.SetBreakpointsResponse, args: DebugProtocol.SetBreakpointsArguments): void {
        try {
            const finalBreakpoints = [];
            if (args.breakpoints && args.source.path) {
                const breakpointLines = args.breakpoints.map(bp => bp.line);
                const validatedBreakpoints = this.runtime.setLineBreakpoints(args.source.path, breakpointLines);
                for (let i = 0; i < breakpointLines.length; i++) {
                    finalBreakpoints.push(new Breakpoint(validatedBreakpoints[i], breakpointLines[i]));
                }
            }
            response.body = {
                breakpoints: finalBreakpoints
            };
        } catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
    }

    protected disconnectRequest(
        response: DebugProtocol.DisconnectResponse,
        _args: DebugProtocol.DisconnectArguments
    ): void {
        // Cleanup and terminate the debug session
        this.sendEvent(new TerminatedEvent());
        this.sendResponse(response);
    }
}
