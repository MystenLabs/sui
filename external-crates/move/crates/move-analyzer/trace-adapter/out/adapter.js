"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || function (mod) {
    if (mod && mod.__esModule) return mod;
    var result = {};
    if (mod != null) for (var k in mod) if (k !== "default" && Object.prototype.hasOwnProperty.call(mod, k)) __createBinding(result, mod, k);
    __setModuleDefault(result, mod);
    return result;
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.MoveDebugSession = void 0;
const debugadapter_1 = require("@vscode/debugadapter");
const path = __importStar(require("path"));
const runtime_1 = require("./runtime");
/**
 * Converts a log level string to a Logger.LogLevel.
 *
 * @param level log level as string
 * @returns log level as Logger.LogLevel
 */
function convertLoggerLogLevel(level) {
    switch (level) {
        case "log" /* LogLevel.Log */:
            return debugadapter_1.Logger.LogLevel.Log;
        case "verbose" /* LogLevel.Verbose */:
            return debugadapter_1.Logger.LogLevel.Verbose;
        default:
            return debugadapter_1.Logger.LogLevel.Stop;
    }
}
class MoveDebugSession extends debugadapter_1.LoggingDebugSession {
    /**
     * We only even have one thread so we can hardcode its ID.
     */
    static THREAD_ID = 1;
    /**
     * DAP-independent runtime maintaining state during
     * trace viewing session.
     */
    runtime;
    /**
     * Handles to create variable scopes and compound variable values.
     */
    variableHandles;
    constructor() {
        super();
        this.setDebuggerLinesStartAt1(false);
        this.setDebuggerColumnsStartAt1(false);
        this.runtime = new runtime_1.Runtime();
        this.variableHandles = new debugadapter_1.Handles();
        // setup event handlers
        this.runtime.on(runtime_1.RuntimeEvents.stopOnStep, () => {
            this.sendEvent(new debugadapter_1.StoppedEvent('step', MoveDebugSession.THREAD_ID));
        });
        this.runtime.on(runtime_1.RuntimeEvents.stopOnLineBreakpoint, () => {
            this.sendEvent(new debugadapter_1.StoppedEvent('breakpoint', MoveDebugSession.THREAD_ID));
        });
        this.runtime.on(runtime_1.RuntimeEvents.stopOnException, (msg) => {
            this.sendEvent(new debugadapter_1.StoppedEvent('exception', MoveDebugSession.THREAD_ID, msg));
        });
        this.runtime.on(runtime_1.RuntimeEvents.end, () => {
            this.sendEvent(new debugadapter_1.TerminatedEvent());
        });
    }
    initializeRequest(response, args) {
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
        // make VS Code send disassemble request
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
        this.sendEvent(new debugadapter_1.InitializedEvent());
    }
    async launchRequest(response, args) {
        debugadapter_1.logger.setup(convertLoggerLogLevel(args.logLevel ?? "none" /* LogLevel.None */), false);
        debugadapter_1.logger.log(`Launching trace viewer for file: ${args.source} and trace: ${args.traceInfo}`);
        try {
            await this.runtime.start(args.source, args.traceInfo, args.stopOnEntry || false);
        }
        catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
        this.sendEvent(new debugadapter_1.StoppedEvent('entry', MoveDebugSession.THREAD_ID));
    }
    threadsRequest(response) {
        response.body = {
            threads: [
                new debugadapter_1.Thread(MoveDebugSession.THREAD_ID, 'Main Thread')
            ]
        };
        this.sendResponse(response);
    }
    stackTraceRequest(response, _args) {
        try {
            const runtimeStack = this.runtime.stack();
            const stack_height = runtimeStack.frames.length;
            response.body = {
                stackFrames: runtimeStack.frames.map(frame => {
                    const fileName = path.basename(frame.file);
                    return new debugadapter_1.StackFrame(frame.id, frame.name, new debugadapter_1.Source(fileName, frame.file), frame.line);
                }).reverse(),
                totalFrames: stack_height,
                optimized_lines: stack_height > 0
                    ? runtimeStack.frames[stack_height - 1].optimizedLines
                    : []
            };
        }
        catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
    }
    /**
     * Gets the scopes for a given frame.
     *
     * @param frameID identifier of the frame scopes are requested for.
     * @returns an array of scopes.
     * @throws Error with a descriptive error message if scopes cannot be retrieved.
     */
    getScopes(frameID) {
        const runtimeStack = this.runtime.stack();
        const frame = runtimeStack.frames.find(frame => frame.id === frameID);
        if (!frame) {
            throw new Error(`No frame found for id: ${frameID} when getting scopes`);
        }
        const scopes = [];
        if (frame.locals.length > 0) {
            for (let i = frame.locals.length - 1; i > 0; i--) {
                const shadowedScopeReference = this.variableHandles.create({ locals: frame.locals[i] });
                const shadowedScope = new debugadapter_1.Scope(`shadowed(${i}): ${frame.name}`, shadowedScopeReference, false);
                scopes.push(shadowedScope);
            }
        }
        // don't have to check if scope 0 exists as it's created whenever a new frame is created
        // and it's never disposed of
        const localScopeReference = this.variableHandles.create({ locals: frame.locals[0] });
        const localScope = new debugadapter_1.Scope(`locals: ${frame.name}`, localScopeReference, false);
        scopes.push(localScope);
        return scopes;
    }
    scopesRequest(response, args) {
        try {
            const scopes = this.getScopes(args.frameId);
            response.body = {
                scopes
            };
        }
        catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
    }
    /**
     * Converts a runtime reference value to a DAP variable.
     *
     * @param value reference value.
     * @param name name of variable containing the reference value.
     * @param type optional type of the variable containing the reference value.
     * @returns a DAP variable.
     * @throws Error with a descriptive error message if conversion fails.
     */
    convertRefValue(value, name, type) {
        const frameID = value.loc.frameID;
        const localIndex = value.loc.localIndex;
        const runtimeStack = this.runtime.stack();
        const frame = runtimeStack.frames.find(frame => frame.id === frameID);
        if (!frame) {
            throw new Error('No frame found for id '
                + frameID
                + ' when converting ref value for local index '
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
                + ' when converting ref value for frame id '
                + frameID);
        }
        return this.convertRuntimeValue(local.value, name, type);
    }
    /**
     * Converts a runtime value to a DAP variable.
     *
     * @param value variable value
     * @param name variable name
     * @param type optional variable type
     * @returns a DAP variable.
     */
    convertRuntimeValue(value, name, type) {
        if (typeof value === 'string') {
            return {
                name,
                type,
                value,
                variablesReference: 0
            };
        }
        else if (Array.isArray(value)) {
            const compoundValueReference = this.variableHandles.create(value);
            return {
                name,
                type,
                value: '(' + value.length + ')[...]',
                variablesReference: compoundValueReference
            };
        }
        else if ('fields' in value) {
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
                    : datatypeName) + '{...}',
                variablesReference: compoundValueReference
            };
        }
        else {
            return this.convertRefValue(value, name, type);
        }
    }
    /**
     * Converts runtime variables to DAP variables.
     *
     * @param runtimeScope runtime variables scope,
     * @returns an array of DAP variables.
     */
    convertRuntimeVariables(runtimeScope) {
        const variables = [];
        const runtimeVariables = runtimeScope.locals;
        runtimeVariables.forEach(v => {
            if (v) {
                variables.push(this.convertRuntimeValue(v.value, v.name, v.type));
            }
        });
        return variables;
    }
    variablesRequest(response, args) {
        try {
            const variableHandle = this.variableHandles.get(args.variablesReference);
            let variables = [];
            if (variableHandle) {
                if ('locals' in variableHandle) {
                    // we are dealing with a sccope
                    variables = this.convertRuntimeVariables(variableHandle);
                }
                else {
                    // we are dealing with a compound value
                    if (Array.isArray(variableHandle)) {
                        for (let i = 0; i < variableHandle.length; i++) {
                            const v = variableHandle[i];
                            variables.push(this.convertRuntimeValue(v, String(i)));
                        }
                    }
                    else {
                        variableHandle.fields.forEach(([fname, fvalue]) => {
                            variables.push(this.convertRuntimeValue(fvalue, fname));
                        });
                    }
                }
            }
            if (variables.length > 0) {
                response.body = {
                    variables
                };
            }
        }
        catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
    }
    nextRequest(response, _args) {
        let terminate = false;
        try {
            const executionResult = this.runtime.step(/* next */ true, /* stopAtCloseFrame */ false);
            terminate = executionResult === runtime_1.ExecutionResult.TraceEnd;
        }
        catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        if (terminate) {
            this.sendEvent(new debugadapter_1.TerminatedEvent());
        }
        this.sendResponse(response);
    }
    stepInRequest(response, _args) {
        let terminate = false;
        try {
            const executionResult = this.runtime.step(/* next */ false, /* stopAtCloseFrame */ false);
            terminate = executionResult === runtime_1.ExecutionResult.TraceEnd;
        }
        catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        if (terminate) {
            this.sendEvent(new debugadapter_1.TerminatedEvent());
        }
        this.sendResponse(response);
    }
    stepOutRequest(response, _args) {
        let terminate = false;
        try {
            const executionResult = this.runtime.stepOut(/* next */ false);
            terminate = executionResult === runtime_1.ExecutionResult.TraceEnd;
        }
        catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        if (terminate) {
            this.sendEvent(new debugadapter_1.TerminatedEvent());
        }
        this.sendResponse(response);
    }
    continueRequest(response, _args) {
        let terminate = false;
        try {
            const executionResult = this.runtime.continue();
            terminate = executionResult === runtime_1.ExecutionResult.TraceEnd;
        }
        catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        if (terminate) {
            this.sendEvent(new debugadapter_1.TerminatedEvent());
        }
        this.sendResponse(response);
    }
    setBreakPointsRequest(response, args) {
        try {
            const finalBreakpoints = [];
            if (args.breakpoints && args.source.path) {
                const breakpointLines = args.breakpoints.map(bp => bp.line);
                const validatedBreakpoints = this.runtime.setLineBreakpoints(args.source.path, breakpointLines);
                for (let i = 0; i < breakpointLines.length; i++) {
                    finalBreakpoints.push(new debugadapter_1.Breakpoint(validatedBreakpoints[i], breakpointLines[i]));
                }
            }
            response.body = {
                breakpoints: finalBreakpoints
            };
        }
        catch (err) {
            response.success = false;
            response.message = err instanceof Error ? err.message : String(err);
        }
        this.sendResponse(response);
    }
    disconnectRequest(response, _args) {
        // Cleanup and terminate the debug session
        this.sendEvent(new debugadapter_1.TerminatedEvent());
        this.sendResponse(response);
    }
}
exports.MoveDebugSession = MoveDebugSession;
//# sourceMappingURL=adapter.js.map