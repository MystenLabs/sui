import { subtask, types } from "hardhat/config";
import type { Artifact } from "hardhat/types/artifacts";
import { Artifacts as ArtifactsImpl } from "hardhat/internal/artifacts";
import { TASK_COMPILE_GET_COMPILATION_TASKS } from "hardhat/builtin-tasks/task-names";
import { Result, ok, err } from "neverthrow";
import * as Fs from "fs";
import * as Path from "path";
import * as ChildProcess from "child_process";

/***************************************************************************************
 *
 * Wrappers for Result-based Error Handling
 *
 *   Functions in the js standard lib uses execeptions for error handling, of which the
 *   the correctness is hard to reason about. Here are a few wrappers that transform
 *   them into Result-based APIs for easy error handling and chaining.
 *
 **************************************************************************************/
class ChainedError extends Error {
    causes: Error[];

    constructor(message: string, cause?: Error | Error[]) {
        super(message);

        if (cause === undefined) {
            this.causes = [];
        }
        else if (Array.isArray(cause)) {
            this.causes = cause;
        }
        else {
            this.causes = [cause];
        }
    }
}

async function resultifyAsync<T>(f: () => Promise<T>): Promise<Result<T, Error>> {
    try {
        return ok(await f());
    }
    catch (e) {
        if (e instanceof Error) {
            return err(e);
        }
        else {
            throw new Error(`${e} is not an instance of Error -- this should not happen`);
        }
    }
}

function resultify<T>(f: () => T): Result<T, Error> {
    try {
        return ok(f());
    }
    catch (e) {
        if (e instanceof Error) {
            return err(e);
        }
        else {
            throw new Error(`${e} is not an instance of Error -- this should not happen`);
        }
    }
}


async function readTextFile(path: Fs.PathLike): Promise<Result<string, Error>> {
    return resultifyAsync(() => {
        return Fs.promises.readFile(path, { encoding: "utf-8" });
    });
}


async function writeTextFile(path: Fs.PathLike, data: string): Promise<Result<void, Error>> {
    return resultifyAsync(() => {
        return Fs.promises.writeFile(path, data);
    });
}

async function readDir(path: Fs.PathLike): Promise<Result<Fs.Dirent[], Error>> {
    return resultifyAsync(() => {
        return Fs.promises.readdir(path, { withFileTypes: true });
    });
}

async function createDirIfNotExists(path: Fs.PathLike): Promise<Result<string | undefined, Error>> {
    return resultifyAsync(() => {
        return Fs.promises.mkdir(path, { recursive: true });
    });
}

async function executeChildProcess(cmd: string): Promise<[ChildProcess.ExecException | null, string, string]> {
    return new Promise((resolve, _reject) => {
        // TODO: preserve coloring
        let proc = ChildProcess.exec(cmd, (err, stdout, stderr) => {
            resolve([err, stdout, stderr]);
        });

        proc.stdin!.end();
    });
}

/***************************************************************************************
 *
 * Utilities to List Move packages in the Contracts Directory
 *
 *
 **************************************************************************************/
async function isMovePackage(path: Fs.PathLike): Promise<boolean> {
    // TODO: Result-based error handling
    let stats: Fs.Stats = await Fs.promises.stat(path);

    if (stats.isDirectory()) {
        let manifestPath = Path.join(path.toString(), "Move.toml");
        let manifestStats: Fs.Stats = await Fs.promises.stat(manifestPath);

        return manifestStats.isFile();
    }

    return false;
}

async function listMovePackages(contractsPath: Fs.PathLike): Promise<Array<String>> {
    // TODO: Result-based error handling
    let dirs: String[] = await Fs.promises.readdir(contractsPath);

    let promises: Promise<String | null>[] = dirs.map((name, idx_, _arr) => {
        let path = Path.join(contractsPath.toString(), name.toString());
        return isMovePackage(path).then(isMove => isMove ? path : null)
    });

    return (await Promise.all(promises)).filter((path): path is String => path !== null)
}

/***************************************************************************************
 *
 * Build
 *
 *   Functions to build Move packages using the `move` executable.
 *
 **************************************************************************************/
async function locateMoveExecutablePath(): Promise<Result<string, Error>> {
    let [e, stdout, _stderr] = await executeChildProcess("which move");

    if (e !== null) {
        return err(e);
    }

    console.assert(stdout !== "");
    let lines: string[] = stdout.split(/\r?\n/);
    return ok(lines[0]);
}


class MoveBuildError {
    exec_err: ChildProcess.ExecException;
    // TODO: right now, `move build` outputs its build errors to stdout instead of stderr.
    // This may not be ideal and we may want to fix it and then revisit the error definition here.
    stdout: string;
    stderr: string;

    constructor(exec_err: ChildProcess.ExecException, stdout: string, stderr: string) {
        this.exec_err = exec_err;
        this.stdout = stdout;
        this.stderr = stderr;
    }
}

async function movePackageBuild(movePath: string, packagePath: string): Promise<Result<void, MoveBuildError>> {
    let cmd = `${movePath} build --path ${packagePath} --arch ethereum`;

    let [e, stdout, stderr] = await executeChildProcess(cmd);

    if (e !== null) {
        return err(new MoveBuildError(e, stdout, stderr));
    }

    return ok(undefined);
}

/***************************************************************************************
 *
 * Artifact Generation
 *
 *   Functions to generate hardhat artifacts from the outputs of the Move compiler
 *   toolchain.
 *
 **************************************************************************************/
async function loadAbi(packagePath: string, contractName: string): Promise<Result<any, ChainedError>> {
    let abiPath = Path.join(packagePath, "build", "evm", `${contractName}.abi.json`);

    let readFileRes = await readTextFile(abiPath);
    if (readFileRes.isErr()) {
        return err(new ChainedError(`Failed to load ABI from ${abiPath}`, readFileRes.error));
    }
    let content = readFileRes.value;

    let jsonParseRes = resultify(() => JSON.parse(content));
    if (jsonParseRes.isErr()) {
        return err(new ChainedError(`Failed to decode ABI -- invalid JSON: ${content}`, jsonParseRes.error));
    }
    return ok(jsonParseRes.value);
}

async function loadBytecode(packagePath: string, contrantName: string): Promise<Result<string, ChainedError>> {
    let bytecodePath = Path.join(packagePath, "build", "evm", `${contrantName}.bin`);

    let readFileRes = await readTextFile(bytecodePath);
    if (readFileRes.isErr()) {
        return err(new ChainedError(`Failed to load bytecode from ${bytecodePath}`, readFileRes.error));
    }

    return ok(readFileRes.value);
}

async function listCompiledContracts(packagePath: string): Promise<Result<string[], ChainedError>> {
    let path = Path.join(packagePath, "build", "evm");

    let readDirRes = await readDir(path);
    if (readDirRes.isErr()) {
        return err(new ChainedError(`Failed to list compiled contracts in ${path}`, readDirRes.error));
    }
    let entries = readDirRes.value;

    let info = [];
    for (let entry of entries) {
        if (entry.isFile()) {
            // REVIEW: can this throw?
            let parsed = Path.parse(entry.name);

            if (parsed.ext == ".bin") {
                info.push(parsed.name);
            }
        }
    }
    return ok(info);
}

async function generateArtifact(hardhatRootPath: string, packagePath: string, contractName: string): Promise<Result<Artifact, ChainedError>> {
    let [loadbytecodeRes, loadAbiRes] = await Promise.all([loadBytecode(packagePath, contractName), loadAbi(packagePath, contractName)]);

    if (loadbytecodeRes.isErr()) {
        return err(loadbytecodeRes.error);
    }

    if (loadAbiRes.isErr()) {
        return err(loadAbiRes.error);
    }

    let bytecode = loadbytecodeRes.value;
    if (!bytecode.startsWith("0x")) {
        bytecode = "0x" + bytecode;
    }
    let abi = loadAbiRes.value;

    let sourcePath = Path.relative(hardhatRootPath, packagePath);

    let artifact: Artifact = {
        "_format": "hh-move-artifact-1",
        "contractName": contractName,
        "sourceName": sourcePath,
        "abi": abi,
        "bytecode": bytecode,
        "deployedBytecode": bytecode,
        "linkReferences": {},
        "deployedLinkReferences": {}
    };

    return ok(artifact);
}

async function generateArtifactsForPackage(hardhatRootPath: string, packagePath: string): Promise<Result<Artifact[], ChainedError>> {
    let listRes = await listCompiledContracts(packagePath);
    if (listRes.isErr()) {
        return err(new ChainedError(`Failed to list compiled contracts in ${packagePath}`, listRes.error));
    }
    let contractNames = listRes.value;

    let genResults = await Promise.all(contractNames.map(contractName => generateArtifact(hardhatRootPath, packagePath, contractName)));

    let errors = [];
    let artifacts = [];
    for (let res of genResults) {
        if (res.isErr()) {
            errors.push(res.error);
        }
        else {
            artifacts.push(res.value);
        }
    }

    if (errors.length > 0) {
        return err(new ChainedError(`Failed to generate artifacts for ${packagePath}`, errors));
    }

    return ok(artifacts);
}

async function buildPackageAndGenerateArtifacts(movePath: string, hardhatRootPath: string, packagePath: string): Promise<Result<Artifact[], MoveBuildError | ChainedError>> {
    let buildRes = await movePackageBuild(movePath, packagePath);
    if (buildRes.isErr()) {
        let e = buildRes.error;
        console.log(`\nFailed to build ${packagePath}\n${e.stdout}${e.stderr}`);
        return err(e);
    }

    let genArtifactsRes = await generateArtifactsForPackage(hardhatRootPath, packagePath);
    if (genArtifactsRes.isErr()) {
        let e = genArtifactsRes.error;
        console.log(`Failed to build ${packagePath}\n${e}`);
        return err(genArtifactsRes.error);
    }

    console.log(`Successfully built ${packagePath}`);

    return ok(genArtifactsRes.value);
}

/***************************************************************************************
 *
 * Move Compile Subtask (Entrypoint)
 *
 *   This adds a new subtask "compile:move" which is added to the queue when one runs
 *   `npx hardhat compile`. This task will build all the move contracts using the `move`
 *   executable and generate the artifacts hardhat requires for testing and deployment.
 *
 **************************************************************************************/
const TASK_COMPILE_MOVE: string = "compile:move";

subtask(
    TASK_COMPILE_GET_COMPILATION_TASKS,
    async (_, __, runSuper): Promise<string[]> => {
        const otherTasks = await runSuper();
        return [...otherTasks, TASK_COMPILE_MOVE];
    }
);

subtask(TASK_COMPILE_MOVE)
    .addParam("quiet", undefined, undefined, types.boolean)
    .setAction(async ({ quiet }: { quiet: boolean }, { artifacts, config, run }) => {
        let packagePaths: String[] = await listMovePackages(Path.join(config.paths.root, "contracts"));

        if (packagePaths.length == 0) {
            console.log("No Move contracts to compile");
            return;
        }

        let plural = packagePaths.length == 1 ? "" : "s";
        console.log("Building %d Move package%s...", packagePaths.length, plural);

        let locateRes = await locateMoveExecutablePath();
        if (locateRes.isErr()) {
            console.log("Failed to locate the `move` executable.");
            console.log(locateRes.error);
            return;
        }
        let movePath = locateRes.value;

        let buildResults = await Promise.all(packagePaths.map(path => buildPackageAndGenerateArtifacts(movePath, config.paths.root, path.toString())));

        let failedToBuildAll = false;
        console.assert(packagePaths.length == buildResults.length);
        for (let idx in packagePaths) {

            let packagePathRel = Path.relative(config.paths.root, packagePaths[idx].toString());

            let res = buildResults[idx];

            if (res.isOk()) {
                let contractNames = [];

                for (let artifact of res.value) {
                    contractNames.push(artifact.contractName);
                    // TODO: error handling
                    await artifacts.saveArtifactAndDebugFile(artifact);
                }

                // TODO: write this in a better way
                const artifactsImpl = artifacts as ArtifactsImpl;
                artifactsImpl.addValidArtifacts([{ sourceName: packagePathRel, artifacts: contractNames }]);
            }
            else {
                failedToBuildAll = true;
            }
        }

        if (failedToBuildAll) {
            // TODO: terminate gracefully
            throw new Error("Failed to build one or more Move packages");
        }
    })

module.exports = {};
