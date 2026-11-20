// The part of @aphrody/x (aphrody-labs/aphrody, packages/x) that this engine uses. The published 1.3.0 source imports its
// GraphQL catalog from outside the package (`../../../../src/x/data/x-graphql-catalog.json`), which no consumer can resolve, so
// the root tsconfig maps `@aphrody/x` to this file instead of type-checking the dependency's sources.
export interface XSearchToolArgs {
  query: string;
  count?: number;
  mode?: "Latest" | "Top";
}
export interface XProfileToolArgs {
  handle: string;
}
export interface XTweetsToolArgs {
  handle: string;
  count?: number;
}
export interface XNewsToolArgs {
  count?: number;
}

/** Fulfils the x_* function-tool calls with the native X client (the cookie session comes from the saved session or the environment). */
export class XTools {
  constructor(source?: string | object);
  search(args: XSearchToolArgs): Promise<unknown>;
  profile(args: XProfileToolArgs): Promise<unknown>;
  whoami(): Promise<unknown>;
  tweets(args: XTweetsToolArgs): Promise<unknown>;
  news(args?: XNewsToolArgs): Promise<unknown>;
  execute(name: string, args: unknown): Promise<unknown>;
}

interface FunctionToolDef<Name extends string> {
  type: "function";
  function: {
    name: Name;
    description: string;
    parameters: {
      type: string;
      properties: Record<string, unknown>;
      required?: string[];
    };
  };
}
export const xSearchToolDef: FunctionToolDef<"x_search">;
export const xProfileToolDef: FunctionToolDef<"x_profile">;
export const xWhoamiToolDef: FunctionToolDef<"x_whoami">;
export const xTweetsToolDef: FunctionToolDef<"x_tweets">;
export const xNewsToolDef: FunctionToolDef<"x_news">;
export const xNativeTools: readonly [
  typeof xSearchToolDef,
  typeof xProfileToolDef,
  typeof xNewsToolDef,
  typeof xTweetsToolDef,
  typeof xWhoamiToolDef,
];
