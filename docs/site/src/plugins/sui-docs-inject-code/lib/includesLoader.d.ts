/// <reference types="node" />
interface Loader extends Function {
    (this: any, source: string): string | Buffer | void | undefined;
}
declare const markdownLoader: Loader;
export default markdownLoader;
//# sourceMappingURL=includesLoader.d.ts.map