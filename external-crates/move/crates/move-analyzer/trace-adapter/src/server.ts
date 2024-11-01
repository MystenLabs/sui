import { DebugSession } from '@vscode/debugadapter';
import { MoveDebugSession } from './adapter';

// Run the MoveDebugSession debug adapter using stdio
DebugSession.run(MoveDebugSession);
