"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const debugadapter_1 = require("@vscode/debugadapter");
const adapter_1 = require("./adapter");
// Run the MoveDebugSession debug adapter using stdio
debugadapter_1.DebugSession.run(adapter_1.MoveDebugSession);
//# sourceMappingURL=server.js.map