import { existsSync, readFileSync, readdirSync, readlinkSync, type Dirent } from "node:fs";
import { join } from "node:path";

/**
 * Dependency hygiene findings.
 *
 * Shrike's core product is the verified deletion of dead source. Dependencies
 * are a separate question: a package can be unused even when every source file
 * imports something. This module answers that question conservatively.
 *
 * A dependency is reported only when it is not:
 *   - imported by any analyzed module in its package,
 *   - invoked by a package script or `bin` name,
 *   - referenced by a recognized tool config (eslint, postcss, tailwind, ...),
 *   - a peer/optional dependency or a local workspace protocol dependency,
 *   - a `@types/*` package (compiler context is not import evidence).
 *
 * Anything the analysis cannot prove is left alone. The report is an aid, not
 * a deletion plan; `shrike prune` never removes dependencies.
 */

export interface SnapshotModule {
  path: string;
  specifiers: string[];
  /** Specifiers the resolver classified as external packages. */
  external?: string[];
  /** Module specifiers referenced from type positions only. */
  typeRefs?: string[];
}

export interface SnapshotFile {
  path: string;
}

export interface SnapshotEntry {
  symbol: string;
  framework: string;
}

export interface SnapshotSymbol {
  key: string;
  kind?: string;
}

export interface SnapshotLike {
  files?: SnapshotFile[];
  modules?: SnapshotModule[];
  entries?: SnapshotEntry[];
  /** Symbol keys reachable from an entry. */
  live?: string[];
  /** Symbol records, used to distinguish a dead module from an unparsed one. */
  symbols?: SnapshotSymbol[];
}

export type DependencyKind =
  | "dependencies"
  | "devDependencies"
  | "optionalDependencies"
  | "peerDependencies";

export interface UnusedDependency {
  name: string;
  kind: DependencyKind;
  reason: string;
}

export interface UnlistedDependency {
  name: string;
  importedBy: string[];
  specifiers: string[];
}

export interface PackageDependencyReport {
  name: string;
  directory: string;
  unused: UnusedDependency[];
  unlisted: UnlistedDependency[];
  /** Packages imported directly but provided by a declared dependency. */
  transitive: UnlistedDependency[];
  counts: {
    dependencies: number;
    devDependencies: number;
    used: number;
  };
}

export interface DependencyReport {
  packages: PackageDependencyReport[];
  summary: {
    packages: number;
    unused: number;
    unlisted: number;
    transitive: number;
    unusedByKind: Record<DependencyKind, number>;
  };
}

interface PackageJson {
  name?: string;
  scripts?: Record<string, string>;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  bin?: string | Record<string, string>;
}

interface PackageContext {
  name: string;
  directory: string;
  json: PackageJson;
  files: string[];
  modules: SnapshotModule[];
  frameworks: Set<string>;
}

const DEPENDENCY_KINDS: DependencyKind[] = [
  "dependencies",
  "devDependencies",
  "optionalDependencies",
  "peerDependencies",
];

const IGNORED_DIRECTORIES = new Set([
  "node_modules",
  ".git",
  ".next",
  ".shrike",
  "dist",
  "build",
  "out",
  "coverage",
  ".turbo",
  ".vercel",
  "vendor",
]);

// Standalone examples and shared test infrastructure are executed directly or
// belong to a nested package. Their imports are not the parent package's
// dependency declarations.
const STANDALONE_PREFIXES = [
  "examples/",
  "example/",
  "demo/",
  "playground/",
  "playgrounds/",
  "benchmarks/",
  "jest/",
  ".storybook/",
];

const CONFIG_FILE = /(^|\/)(?:[^/]*\.config\.(?:[cm]?[jt]sx?|json)|gatsby-(?:config|node|browser|ssr|shared)\.[cm]?[jt]s|\.(?:eslintrc|prettierrc|babelrc|commitlintrc|lintstagedrc|stylelintrc|swcrc|npmrc)(?:\.[cm]?[jt]sx?|\.json)?|eslint\.config\.[cm]?[jt]s|tsconfig(?:\.[^/]*)?\.json|jsconfig\.json|pnpm-workspace\.yaml|turbo\.json|biome\.jsonc?|renovate\.json|nx\.json|lerna\.json)$/;

const ASSET_EXTENSION = /\.(?:css|scss|sass|less|styl|svg|png|jpe?g|gif|webp|avif|ico|bmp|woff2?|ttf|otf|eot|mp4|webm|mp3|wav|pdf|wasm)$/i;

const NODE_BUILTINS = new Set([
  "assert", "async_hooks", "buffer", "child_process", "cluster", "console",
  "constants", "crypto", "dgram", "diagnostics_channel", "dns", "domain",
  "events", "fs", "http", "http2", "https", "inspector", "module", "net",
  "os", "path", "perf_hooks", "process", "punycode", "querystring",
  "readline", "repl", "stream", "string_decoder", "sys", "timers", "tls",
  "trace_events", "tty", "url", "util", "v8", "vm", "wasi", "worker_threads",
  "zlib", "test",
]);

// The analyzer itself is provided by the runner. A project's Shrike config may
// import the tool's types without declaring the tool as a package dependency.
const TOOL_PACKAGES = new Set(["getshrike", "shrike", "@shrike/cli"]);

// Ecosystem requirements that cannot be read from an installed manifest (the
// repository may not have been installed, or the dependency may be a peer).
// These are conservative: they only suppress findings, never create them.
const KNOWN_REQUIREMENTS: Record<string, string[]> = {
  "@mui/material": ["@emotion/react", "@emotion/styled"],
  "@swc/jest": ["@swc/core"],
  "@swc-node/register": ["@swc/core"],
  "jotai-optics": ["optics-ts"],
  "tailwindcss": ["postcss"],
};

// A framework entry proves the package is part of the runtime even when no
// module imports it directly (JSX transforms, router conventions, CLI-loaded
// files). These only suppress findings for packages that are declared.
const FRAMEWORK_PACKAGES: Record<string, string[]> = {
  "next-app": ["next", "react", "react-dom"],
  "next-pages": ["next", "react", "react-dom"],
  react: ["react"],
  remix: ["@remix-run/react", "react", "react-dom"],
  tanstack: ["@tanstack/react-router", "react", "react-dom"],
  expo: ["expo", "expo-router", "react", "react-native"],
  sveltekit: ["@sveltejs/kit", "svelte"],
  astro: ["astro"],
  nuxt: ["nuxt", "vue"],
  gatsby: ["gatsby", "react", "react-dom"],
};

function readPackageJson(directory: string): PackageJson | null {
  const path = join(directory, "package.json");
  if (!existsSync(path)) return null;
  try {
    const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return null;
    return parsed as PackageJson;
  } catch {
    return null;
  }
}

function discoverPackageDirectories(root: string, cache: ScanCache): string[] {
  const directories = new Set<string>();
  const rootEntries = readdirCached(root, "", cache);
  if (rootEntries.some((entry) => entry.name === "package.json" && entry.isFile())) {
    directories.add("");
  }
  const walk = (directory: string, depth: number): void => {
    if (depth > 3) return;
    const entries = readdirCached(root, directory, cache);
    for (const entry of entries) {
      if (!entry.isDirectory() || IGNORED_DIRECTORIES.has(entry.name) || entry.name.startsWith(".")) {
        continue;
      }
      const relative = directory === "" ? entry.name : `${directory}/${entry.name}`;
      const childEntries = readdirCached(root, relative, cache);
      if (childEntries.some((child) => child.name === "package.json" && child.isFile())) {
        directories.add(relative);
        // A workspace package normally owns its subtree; keep descending only
        // one level so nested examples are still found without walking the
        // whole repository.
      }
      walk(relative, depth + 1);
    }
  };
  walk("", 0);
  return [...directories].sort((a, b) => a.length - b.length || a.localeCompare(b));
}

function packageNameOf(specifier: string): string | null {
  if (specifier.startsWith(".") || specifier.startsWith("/")) return null;
  if (specifier.startsWith("node:")) return null;
  if (specifier.startsWith("@/") || specifier.startsWith("~/") || specifier.startsWith("#")) return null;
  if (specifier.startsWith("virtual:") || specifier.startsWith("data:") || specifier.startsWith("http:")) return null;
  if (ASSET_EXTENSION.test(specifier)) return null;
  const parts = specifier.split("/");
  const name = parts[0]?.startsWith("@")
    ? parts.length >= 2
      ? `${parts[0]}/${parts[1]}`
      : null
    : parts[0] || null;
  // npm package names are lowercase. Bare aliases from Deno import maps and
  // similar tool maps (`YAML`, `Files`) are capitalized and are not packages.
  if (!name || name !== name.toLowerCase()) return null;
  return name;
}

function isBuiltin(name: string): boolean {
  return NODE_BUILTINS.has(name.replace(/^node:/, ""));
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function shellTokens(command: string): string[] {
  return command
    .split(/[\s;&|(){}[\]<>]+/)
    .map((token) => token.trim().replace(/^['"]|['"]$/g, ""))
    .map((token) => token.replace(/^--?[^=]+=/, ""))
    .filter((token) => token.length > 0);
}

/** Per-scan caches for filesystem evidence. Dependency analysis reads the same
 * manifests and config files once per declared package, so memoizing them is
 * the difference between a few hundred file reads and a few thousand. */
interface ScanCache {
  texts: Map<string, string | null>;
  manifests: Map<string, PackageJson | null>;
  bins: Map<string, string[]>;
  binLinks: Map<string, Map<string, string>>;
  dirents: Map<string, Dirent[]>;
}

function newScanCache(): ScanCache {
  return {
    texts: new Map(),
    manifests: new Map(),
    bins: new Map(),
    binLinks: new Map(),
    dirents: new Map(),
  };
}

/** Directory listings shared by the package, config, and source walks. */
function readdirCached(root: string, directory: string, cache: ScanCache): Dirent[] {
  const cached = cache.dirents.get(directory);
  if (cached) return cached;
  let entries: Dirent[] = [];
  try {
    entries = readdirSync(join(root, directory), { withFileTypes: true });
  } catch {
    // A missing or unreadable directory has no entries.
  }
  cache.dirents.set(directory, entries);
  return entries;
}

function readTextCached(root: string, file: string, cache: ScanCache): string | null {
  const cached = cache.texts.get(file);
  if (cached !== undefined) return cached;
  let text: string | null = null;
  try {
    text = readFileSync(join(root, file), "utf8");
  } catch {
    // An unreadable file stays null so repeated lookups do not retry.
  }
  cache.texts.set(file, text);
  return text;
}

/** Executable name -> link target for both node_modules/.bin directories. */
function binLinks(
  root: string,
  directory: string,
  cache: ScanCache,
): Map<string, string> {
  const key = directory;
  const cached = cache.binLinks.get(key);
  if (cached) return cached;
  const links = new Map<string, string>();
  for (const nodeModules of [
    join(root, directory, "node_modules"),
    join(root, "node_modules"),
  ]) {
    const binDir = join(nodeModules, ".bin");
    if (!existsSync(binDir)) continue;
    try {
      for (const entry of readdirSync(binDir)) {
        if (links.has(entry)) continue;
        try {
          links.set(entry, readlinkSync(join(binDir, entry)).replace(/\\/g, "/"));
        } catch {
          // A dangling link cannot attribute a binary.
        }
      }
    } catch {
      // An unreadable bin directory cannot prove a binary name.
    }
  }
  cache.binLinks.set(key, links);
  return links;
}

function binNames(
  root: string,
  directory: string,
  packageName: string,
  cache: ScanCache,
): string[] {
  const key = `${directory}\0${packageName}`;
  const cached = cache.bins.get(key);
  if (cached) return cached;
  const json = installedPackageJson(root, directory, packageName, cache);
  const output: string[] = [];
  if (json) {
    if (typeof json.bin === "string") {
      output.push(packageName.split("/").pop() ?? packageName);
    } else if (json.bin && typeof json.bin === "object") {
      output.push(...Object.keys(json.bin));
    }
  }
  // pnpm exposes executables through `.bin` even when the package manifest is
  // not reachable from a direct `node_modules/<name>` path. Read the link
  // target to attribute the binary to the package that provides it.
  for (const [entry, target] of binLinks(root, directory, cache)) {
    if (output.includes(entry)) continue;
    if (
      target.includes(`/${packageName}/`) ||
      target.startsWith(`${packageName}/`) ||
      target.includes(`node_modules/${packageName}/`)
    ) {
      output.push(entry);
    }
  }
  cache.bins.set(key, output);
  return output;
}

function isDeclared(name: string, declarations: Map<string, DependencyKind[]>): boolean {
  return declarations.has(name);
}

const REQUIREMENT_KINDS = ["dependencies", "peerDependencies", "optionalDependencies"] as const;

const pnpmEntriesCache = new Map<string, string[]>();

function pnpmEntries(nodeModules: string): string[] {
  const cached = pnpmEntriesCache.get(nodeModules);
  if (cached) return cached;
  let entries: string[] = [];
  try {
    entries = readdirSync(join(nodeModules, ".pnpm"));
  } catch {
    // No pnpm store.
  }
  pnpmEntriesCache.set(nodeModules, entries);
  return entries;
}

function pnpmManifest(nodeModules: string, name: string): PackageJson | null {
  const prefix = name.replaceAll("/", "+");
  for (const entry of pnpmEntries(nodeModules)) {
    if (!entry.startsWith(`${prefix}@`)) continue;
    const path = join(nodeModules, ".pnpm", entry, "node_modules", name, "package.json");
    if (!existsSync(path)) continue;
    try {
      const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
      if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)) {
        return parsed as PackageJson;
      }
    } catch {
      // A malformed store manifest cannot prove a requirement.
    }
  }
  return null;
}

/**
 * Read an installed package manifest, preferring the package-local tree and
 * falling back to the hoisted root tree used by npm/pnpm workspaces, then to
 * the pnpm content-addressed store when the package is not linked.
 */
function installedPackageJson(
  root: string,
  directory: string,
  name: string,
  cache: ScanCache,
): PackageJson | null {
  const key = `${directory}\0${name}`;
  const cached = cache.manifests.get(key);
  if (cached !== undefined) return cached;
  const localModules = join(root, directory, "node_modules");
  const rootModules = join(root, "node_modules");
  const candidates = [
    join(localModules, name, "package.json"),
    join(rootModules, name, "package.json"),
  ];
  let result: PackageJson | null = null;
  for (const path of candidates) {
    if (!existsSync(path)) continue;
    try {
      const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
      if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)) {
        result = parsed as PackageJson;
        break;
      }
    } catch {
      // A malformed installed manifest cannot prove a requirement.
    }
  }
  result ??= pnpmManifest(localModules, name) ?? pnpmManifest(rootModules, name);
  cache.manifests.set(key, result);
  return result;
}

function relevantConfigFiles(
  root: string,
  context: PackageContext,
  directories: string[],
  cache: ScanCache,
): string[] {
  const files = new Set<string>();
  for (const file of context.files) {
    if (CONFIG_FILE.test(file)) files.add(file);
  }
  const scope = context.directory === "" ? directories : [context.directory];
  for (const directory of scope) {
    for (const entry of readdirCached(root, directory, cache)) {
      if (!entry.isFile()) continue;
      const relative = directory === "" ? entry.name : `${directory}/${entry.name}`;
      if (CONFIG_FILE.test(relative)) files.add(relative);
    }
  }
  return [...files].sort();
}

// Names that are also common source/config file extensions. A config glob
// (`*.test.tsx`, `*.{js,ts,tsx}`) is not evidence that a package with that
// name is used.
const FILE_EXTENSION_NAMES = new Set([
  "ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts",
  "json", "jsonc", "css", "scss", "sass", "less", "styl",
  "vue", "svelte", "astro", "mdx", "md", "html", "yaml", "yml",
]);

function configMentions(
  root: string,
  file: string,
  name: string,
  cache: ScanCache,
): boolean {
  const text = readTextCached(root, file, cache);
  if (text === null) return false;
  // Most dependencies never appear in a given config. The literal check is
  // much cheaper than compiling and running the boundary regex below.
  if (!text.includes(name)) return false;
  // `/` is a boundary: a scoped package is often referenced inside a path
  // (`node_modules/@biomejs/biome/schema.json`).
  const pattern = new RegExp(
    `(^|[^A-Za-z0-9_-])${escapeRegExp(name)}($|[^A-Za-z0-9_-])`,
    "g",
  );
  for (const match of text.matchAll(pattern)) {
    const start = (match.index ?? 0) + (match[1]?.length ?? 0);
    const before = text[start - 1] ?? "";
    const after = text[start + name.length] ?? "";
    if (FILE_EXTENSION_NAMES.has(name)) {
      // `*.test.tsx`, `**/*.tsx`, `\\.tsx?$`
      if (before === "." || before === "*" || after === "?") continue;
      // `*.{js,ts,tsx}`
      if ((before === "," || before === "{") && (after === "}" || after === ",")) continue;
    }
    return true;
  }
  return false;
}

/** Names a config file may use for a dependency (`eslint-config-next` -> `next`). */
function dependencyAliases(name: string): string[] {
  const aliases = new Set([name]);
  for (const prefix of [
    "eslint-config-",
    "eslint-plugin-",
    "eslint-import-resolver-",
    "prettier-plugin-",
  ]) {
    if (name.startsWith(prefix)) aliases.add(name.slice(prefix.length));
  }
  return [...aliases];
}

// Ambient type packages whose globals are visible without an explicit
// `types` entry. These are never reported unused.
const GLOBAL_TYPE_PACKAGES = new Set([
  "@types/node",
  "@types/react",
  "@types/react-dom",
  "@types/jest",
  "@types/mocha",
  "@types/chai",
  "@types/jasmine",
  "@types/qunit",
  "@types/webpack-env",
]);

/** `@types/babel__core` -> `@babel/core`; `@types/foo` -> `foo`. */
function typesTarget(name: string): string | null {
  if (!name.startsWith("@types/")) return null;
  const rest = name.slice("@types/".length);
  if (rest.includes("__")) return `@${rest.replace("__", "/")}`;
  return rest;
}

/** Package names listed in `compilerOptions.types` across tsconfig files. */
function compilerTypeNames(
  root: string,
  files: string[],
  cache: ScanCache,
): Set<string> {
  const names = new Set<string>();
  for (const file of files) {
    if (!/(^|\/)tsconfig(?:\.[^/]*)?\.json$/.test(file)) continue;
    const text = readTextCached(root, file, cache);
    if (text === null) continue;
    const match = /"types"\s*:\s*\[([^\]]*)\]/.exec(text);
    if (!match) continue;
    const list = match[1];
    if (!list) continue;
    for (const item of list.matchAll(/"([^"]+)"/g)) {
      const name = item[1];
      if (name) names.add(name);
    }
  }
  return names;
}

function mentionsAny(
  root: string,
  file: string,
  aliases: string[],
  cache: ScanCache,
): boolean {
  return aliases.some((alias) => configMentions(root, file, alias, cache));
}

/**
 * Next.js loads `babel-plugin-react-compiler` when `reactCompiler` is enabled,
 * but the package name never appears in the config. Next.js 16 uses the
 * top-level option; Next.js 15 used `experimental.reactCompiler`. A truthy
 * boolean or an options object counts; `false`, dynamic values, and comments
 * that do not match the option shape do not.
 */
function nextReactCompilerEnabled(
  root: string,
  files: string[],
  cache: ScanCache,
): boolean {
  for (const file of files) {
    if (!/(^|\/)next\.config\.[cm]?[jt]s$/.test(file)) continue;
    const text = readTextCached(root, file, cache);
    if (text !== null && /\breactCompiler\s*:\s*(?:true|\{)/.test(text)) return true;
  }
  return false;
}

function fileNameMentions(file: string, aliases: string[]): boolean {
  const base = file.split("/").pop()?.toLowerCase() ?? "";
  return aliases.some((alias) => {
    const name = alias.toLowerCase();
    if (!base.includes(name)) return false;
    // A config file's extension is not evidence that an extension-named
    // package is used (`tsconfig.json` does not use the `json` package).
    if (FILE_EXTENSION_NAMES.has(name) && base.endsWith(`.${name}`)) return false;
    return true;
  });
}

interface WalkOptions {
  extensions: RegExp;
  maxDepth?: number;
  includeHidden?: boolean;
}

/** Bounded walk used for source/config surfaces the parser does not cover. */
function walkFiles(
  root: string,
  directory: string,
  options: WalkOptions,
  cache: ScanCache,
): string[] {
  const output: string[] = [];
  const maxDepth = options.maxDepth ?? 5;
  const walk = (relative: string, depth: number): void => {
    if (depth > maxDepth) return;
    for (const entry of readdirCached(root, relative, cache)) {
      const path = relative === "" ? entry.name : `${relative}/${entry.name}`;
      if (entry.isDirectory()) {
        if (IGNORED_DIRECTORIES.has(entry.name)) continue;
        if (!options.includeHidden && entry.name.startsWith(".")) continue;
        walk(path, depth + 1);
      } else if (entry.isFile() && options.extensions.test(entry.name)) {
        output.push(path);
      }
    }
  };
  walk(directory, 0);
  return output.sort();
}

/**
 * Shared test and tool infrastructure: Jest helpers, Storybook configuration,
 * and config files in subdirectories. These files execute tools but are not
 * part of a package's module graph, so they are scanned by content instead.
 */
function toolConfigFiles(root: string, directory: string, cache: ScanCache): string[] {
  const prefix = directory === "" ? "" : `${directory}/`;
  const starts = new Set<string>();
  for (const name of ["jest", ".storybook"]) {
    starts.add(`${prefix}${name}`);
    // Shared root tooling applies to every workspace package.
    if (directory !== "") starts.add(name);
  }
  const files = new Set<string>();
  for (const start of starts) {
    if (!existsSync(join(root, start))) continue;
    for (const file of walkFiles(root, start, {
      extensions: /\.(?:[cm]?[jt]sx?|json)$/i,
      maxDepth: 4,
      includeHidden: true,
    }, cache)) {
      files.add(file);
    }
  }
  for (const file of walkFiles(root, directory, {
    extensions: CONFIG_FILE,
    maxDepth: 4,
  }, cache)) {
    files.add(file);
  }
  return [...files].sort();
}

/** Files executed by git hooks or shell tooling, not by the module graph. */
function shellFiles(root: string, directory: string, cache: ScanCache): string[] {
  const files = new Set(
    walkFiles(root, directory, { extensions: /\.(?:sh|bash|zsh)$/i, maxDepth: 4 }, cache),
  );
  const hooks = join(root, directory, ".husky");
  const walkHooks = (relative: string): void => {
    let entries: Array<{ isDirectory(): boolean; isFile(): boolean; name: string }>;
    try {
      entries = readdirSync(join(hooks, relative), { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (entry.isDirectory()) {
        if (entry.name === "_") continue;
        walkHooks(relative === "" ? entry.name : `${relative}/${entry.name}`);
      } else if (entry.isFile()) {
        const path = relative === "" ? entry.name : `${relative}/${entry.name}`;
        files.add(`${directory === "" ? "" : `${directory}/`}.husky/${path}`);
      }
    }
  };
  walkHooks("");
  return [...files].sort();
}

/** Package scripts across the package scope (workspace children for a root). */
function scriptTokensFor(
  root: string,
  context: PackageContext,
  directories: string[],
): Set<string> {
  const directoriesInScope =
    context.directory === ""
      ? directories
      : directories.filter(
          (directory) =>
            directory === context.directory || directory.startsWith(`${context.directory}/`),
        );
  const tokens = new Set<string>();
  for (const directory of directoriesInScope) {
    const json = directory === context.directory ? context.json : readPackageJson(join(root, directory));
    for (const script of Object.values(json?.scripts ?? {})) {
      for (const token of shellTokens(script)) tokens.add(token);
    }
  }
  return tokens;
}

function moduleFilesFor(context: PackageContext, allDirectories: string[]): Set<string> {
  const prefix = context.directory === "" ? "" : `${context.directory}/`;
  const nested = allDirectories.filter(
    (directory) =>
      directory !== context.directory &&
      (context.directory === "" ? directory !== "" : directory.startsWith(prefix)),
  );
  const files = new Set<string>();
  for (const module of context.modules) {
    if (prefix !== "" && !module.path.startsWith(prefix)) continue;
    if (context.directory === "" && nested.some((directory) => module.path.startsWith(`${directory}/`))) {
      continue;
    }
    files.add(module.path);
  }
  return files;
}

/**
 * Analyze unused and unlisted dependencies for every discovered package.
 *
 * `ignore` is a repository-wide allowlist of dependency names (for example a
 * runtime tool that is intentionally invoked outside the repository).
 */
export function analyzeDependencies(
  root: string,
  snapshot: SnapshotLike,
  ignore: string[] = [],
): DependencyReport {
  const ignored = new Set(ignore);
  const cache = newScanCache();
  const directories = discoverPackageDirectories(root, cache);
  const allNames = new Set<string>();
  const contexts: PackageContext[] = [];
  const modules = snapshot.modules ?? [];
  const files = (snapshot.files ?? []).map((file) => file.path);
  const entries = snapshot.entries ?? [];
  // A module is live when its `__module__` symbol is reachable. Without
  // liveness data, or when the module symbol is missing (a parse failure), the
  // module stays live: dependency findings must fail closed.
  const liveSymbols = snapshot.live ? new Set(snapshot.live) : null;
  const moduleKeys = new Set(
    (snapshot.symbols ?? [])
      .filter((symbol) => symbol.kind === "module")
      .map((symbol) => symbol.key),
  );
  const moduleIsLive = (path: string): boolean => {
    if (!liveSymbols) return true;
    const key = `${path}#__module__`;
    if (!moduleKeys.has(key)) return true;
    return liveSymbols.has(key);
  };

  for (const directory of directories) {
    const json = readPackageJson(join(root, directory));
    if (!json) continue;
    const name = json.name ?? (directory === "" ? "root" : directory);
    allNames.add(name);
    const prefix = directory === "" ? "" : `${directory}/`;
    const packageModules = modules.filter((module) => {
      if (prefix === "" || module.path.startsWith(prefix)) return true;
      return false;
    });
    const packageFiles = files.filter((file) => prefix === "" || file.startsWith(prefix));
    const frameworks = new Set(
      entries
        .filter((entry) => {
          const file = entry.symbol.split("#")[0] ?? entry.symbol;
          return prefix === "" || file.startsWith(prefix);
        })
        .map((entry) => entry.framework),
    );
    contexts.push({
      name,
      directory,
      json,
      files: packageFiles,
      modules: packageModules,
      frameworks,
    });
  }

  const reports: PackageDependencyReport[] = [];
  for (const context of contexts) {
    const declarations = new Map<string, DependencyKind[]>();
    for (const kind of DEPENDENCY_KINDS) {
      for (const name of Object.keys(context.json[kind] ?? {})) {
        const list = declarations.get(name) ?? [];
        list.push(kind);
        declarations.set(name, list);
      }
    }
    // Alias sets are consulted for every config/CSS/shell file. Derive them
    // once per package instead of rebuilding them inside the file loops.
    const aliasesByName = new Map<string, string[]>();
    for (const name of declarations.keys()) {
      aliasesByName.set(name, dependencyAliases(name));
    }
    const aliasesOf = (name: string): string[] => aliasesByName.get(name) ?? [name];
    const used = new Set<string>();
    const usedBy: Record<string, string[]> = {};
    const mark = (name: string, evidence: string): void => {
      if (!declarations.has(name)) return;
      used.add(name);
      const list = usedBy[name] ?? [];
      list.push(evidence);
      usedBy[name] = list;
    };

    const deadImporters = new Map<string, string[]>();
    const noteDeadImport = (name: string, path: string): void => {
      const list = deadImporters.get(name) ?? [];
      if (!list.includes(path)) list.push(path);
      deadImporters.set(name, list);
    };
    for (const module of context.modules) {
      const live = moduleIsLive(module.path);
      // Mark declared dependencies from every specifier, including workspace
      // siblings that resolve to source instead of being external packages.
      for (const specifier of module.specifiers) {
        const name = packageNameOf(specifier);
        if (!name) continue;
        if (live) mark(name, module.path);
        else noteDeadImport(name, module.path);
      }
      // Type-only references (`import("pkg").Config`, JSDoc `@type`) are
      // dependency evidence even when no runtime module imports the package.
      for (const specifier of module.typeRefs ?? []) {
        const name = packageNameOf(specifier);
        if (!name) continue;
        if (live) mark(name, module.path);
        else noteDeadImport(name, module.path);
      }
    }

    // Framework entries imply their runtime packages even when the package is
    // reached through conventions or a JSX transform instead of an import.
    for (const framework of context.frameworks) {
      for (const name of FRAMEWORK_PACKAGES[framework] ?? []) {
        mark(name, `${framework} entry`);
      }
    }

    const scriptTokens = scriptTokensFor(root, context, directories);
    for (const name of declarations.keys()) {
      const aliases = aliasesOf(name);
      if (aliases.some((alias) => scriptTokens.has(alias))) {
        mark(name, "package.json script");
        continue;
      }
      if (binNames(root, context.directory, name, cache).some((bin) => scriptTokens.has(bin))) {
        mark(name, "package.json script (bin)");
      }
    }

    // Shell scripts and git hooks execute tools outside the module graph.
    const hookTokens = new Set<string>();
    for (const file of shellFiles(root, context.directory, cache)) {
      try {
        for (const token of shellTokens(readFileSync(join(root, file), "utf8"))) {
          hookTokens.add(token);
        }
      } catch {
        // An unreadable hook cannot prove usage.
      }
    }
    for (const name of declarations.keys()) {
      if (used.has(name)) continue;
      const aliases = aliasesOf(name);
      if (aliases.some((alias) => hookTokens.has(alias))) mark(name, "shell script");
      else if (binNames(root, context.directory, name, cache).some((bin) => hookTokens.has(bin))) {
        mark(name, "shell script (bin)");
      }
    }
    if (existsSync(join(root, context.directory, ".husky")) && declarations.has("husky")) {
      mark("husky", ".husky hooks");
    }

    // CSS tool directives (`@plugin`, `@import`, `@apply`) are tool config.
    for (const file of walkFiles(root, context.directory, {
      extensions: /\.(?:css|scss|sass|less|styl|pcss)$/i,
    }, cache)) {
      for (const name of declarations.keys()) {
        if (used.has(name)) continue;
        if (mentionsAny(root, file, aliasesOf(name), cache)) mark(name, file);
      }
    }

    // Config files: content, filename, and package.json tool fields. Shared
    // Jest/Storybook helpers and nested config files execute tools too.
    const configFiles = new Set([
      ...relevantConfigFiles(root, context, directories, cache),
      ...toolConfigFiles(root, context.directory, cache),
    ]);
    for (const file of [...configFiles].sort()) {
      for (const name of declarations.keys()) {
        if (used.has(name)) continue;
        const aliases = aliasesOf(name);
        if (mentionsAny(root, file, aliases, cache) || fileNameMentions(file, aliases)) {
          mark(name, file);
        }
      }
    }
    // Next.js names the React Compiler package by convention, not by string:
    // `reactCompiler: true` (16) or `experimental.reactCompiler` (15) loads
    // `babel-plugin-react-compiler`.
    if (
      declarations.has("babel-plugin-react-compiler") &&
      nextReactCompilerEnabled(root, [...configFiles], cache)
    ) {
      mark("babel-plugin-react-compiler", "next.config reactCompiler");
    }
    for (const key of Object.keys(context.json)) {
      if (declarations.has(key)) mark(key, `package.json ${key}`);
    }

    // Surfaces the parser does not read (Vue, Svelte, Astro) can still import
    // packages; a mention is enough to avoid a false finding. MDX is parsed
    // for its ESM boundary, so prose is not dependency evidence.
    for (const file of walkFiles(root, context.directory, {
      extensions: /\.(?:vue|svelte|astro)$/i,
    }, cache)) {
      for (const name of declarations.keys()) {
        if (used.has(name)) continue;
        if (mentionsAny(root, file, aliasesOf(name), cache)) mark(name, file);
      }
    }

    // The compiler itself is infrastructure: a project that typechecks needs it.
    if (declarations.has("typescript")) mark("typescript", "TypeScript compiler");

    // `importHelpers` emits imports of tslib from the compiler.
    if (declarations.has("tslib")) {
      for (const file of relevantConfigFiles(root, context, directories, cache)) {
        if (!/tsconfig/.test(file)) continue;
        const text = readTextCached(root, file, cache);
        if (text !== null && /"importHelpers"\s*:\s*true/.test(text)) {
          mark("tslib", `${file} importHelpers`);
          break;
        }
      }
    }

    // Framework-provided runtime dependencies that are not in the manifest's
    // dependency lists but are required when the framework is used.
    if (used.has("next") && declarations.has("sharp")) {
      mark("sharp", "required by next");
    }

    // Requirements can be read from installed manifests or, when the
    // repository is not installed, from a small conservative map of ecosystem
    // peers. A used dependency can require another declared dependency: a
    // framework peer (`react-dom` for `next`), a test-runner peer (`vite` for
    // `vitest`), or a tool runtime (`tailwindcss` for
    // `@tailwindcss/postcss`). Removing the required package would be
    // redundant, not provably dead, so usage propagates to a fixpoint.
    const requirementCache = new Map<string, string[]>();
    const requirementsOf = (name: string): string[] => {
      const cached = requirementCache.get(name);
      if (cached) return cached;
      const output = new Set<string>(KNOWN_REQUIREMENTS[name] ?? []);
      const manifest = installedPackageJson(root, context.directory, name, cache);
      if (manifest) {
        for (const kind of REQUIREMENT_KINDS) {
          for (const dependency of Object.keys(manifest[kind] ?? {})) output.add(dependency);
        }
      }
      const requirements = [...output];
      requirementCache.set(name, requirements);
      return requirements;
    };

    let changed = true;
    while (changed) {
      changed = false;
      for (const requirer of [...used].sort()) {
        if (!declarations.has(requirer)) continue;
        for (const dependency of requirementsOf(requirer)) {
          if (!declarations.has(dependency) || used.has(dependency) || ignored.has(dependency)) {
            continue;
          }
          mark(dependency, `required by ${requirer}`);
          changed = true;
        }
      }
    }

    // Type packages are paired with their runtime package. A type package is
    // kept when its target is used, when it provides ambient globals, or when
    // tsconfig lists it explicitly. When the target is declared but unused,
    // both declarations are stale and both are reported.
    const compilerTypes = compilerTypeNames(root, [...configFiles], cache);
    for (const name of declarations.keys()) {
      if (!name.startsWith("@types/")) continue;
      if (GLOBAL_TYPE_PACKAGES.has(name)) {
        mark(name, "TypeScript types");
        continue;
      }
      const short = name.slice("@types/".length);
      if (compilerTypes.has(short) || compilerTypes.has(name)) {
        mark(name, "tsconfig types");
        continue;
      }
      const target = typesTarget(name);
      if (target && declarations.has(target)) {
        if (used.has(target)) mark(name, `types for ${target}`);
        // An unused runtime package makes its type package stale too.
        continue;
      }
      // An orphan type package may provide globals; without a typecheck it is
      // not safe to call it dead.
      mark(name, "TypeScript types");
    }

    // A peer or optional package provided by a declared dependency is not an
    // unlisted dependency: it is expected to come with the dependency. Regular
    // transitive dependencies are not suppressed: importing one directly is a
    // real missing declaration.
    const declaredRequirements = new Set<string>();
    for (const name of declarations.keys()) {
      for (const dependency of KNOWN_REQUIREMENTS[name] ?? []) declaredRequirements.add(dependency);
      const manifest = installedPackageJson(root, context.directory, name, cache);
      if (!manifest) continue;
      for (const kind of ["peerDependencies", "optionalDependencies"] as const) {
        for (const dependency of Object.keys(manifest[kind] ?? {})) {
          declaredRequirements.add(dependency);
        }
      }
    }

    const unused: UnusedDependency[] = [];
    for (const [name, kinds] of [...declarations].sort(([a], [b]) => a.localeCompare(b))) {
      if (ignored.has(name) || used.has(name)) continue;
      const kind = kinds[0] ?? "dependencies";
      if (kind === "peerDependencies" || kind === "optionalDependencies") continue;
      if (kinds.includes("peerDependencies") || kinds.includes("optionalDependencies")) continue;
      const declaration = context.json[kind]?.[name] ?? "";
      if (declaration.startsWith("workspace:") || declaration.startsWith("file:")) continue;
      const dead = [...(deadImporters.get(name) ?? [])].sort();
      const reason =
        dead.length > 0
          ? `only imported by modules with no live symbols: ${dead.slice(0, 3).join(", ")}${
              dead.length > 3 ? `, +${dead.length - 3} more` : ""
            }`
          : "no import, script, bin, or tool-config reference";
      unused.push({ name, kind, reason });
    }

    const moduleFiles = moduleFilesFor(context, directories);
    // A nested workspace package can import a dependency hoisted to an
    // ancestor package. That is not an unlisted dependency.
    const ancestorDeclarations = new Set<string>();
    for (const other of contexts) {
      if (other.directory === context.directory) continue;
      const ancestor =
        other.directory === "" || context.directory.startsWith(`${other.directory}/`);
      if (!ancestor) continue;
      for (const kind of DEPENDENCY_KINDS) {
        for (const name of Object.keys(other.json[kind] ?? {})) ancestorDeclarations.add(name);
      }
    }
    const byName = new Map<string, { importedBy: string[]; specifiers: string[] }>();
    const transitiveByName = new Map<string, { importedBy: string[]; specifiers: string[] }>();
    for (const module of context.modules) {
      if (!moduleFiles.has(module.path)) continue;
      if (
        context.directory === "" &&
        STANDALONE_PREFIXES.some((prefix) => module.path.startsWith(prefix))
      ) {
        continue;
      }
      const specifiers = new Set(module.external ?? module.specifiers);
      // Type-only references are usage evidence above, but they are not
      // unlisted evidence: a JSDoc `import("pkg")` in a tool config is often
      // satisfied by the tool's own transitive types, and a genuinely missing
      // type package is caught by the typecheck.
      for (const specifier of specifiers) {
        const name = packageNameOf(specifier);
        if (!name || isBuiltin(name) || ignored.has(name)) continue;
        if (name === context.name || TOOL_PACKAGES.has(name)) continue;
        if (isDeclared(name, declarations) || ancestorDeclarations.has(name)) {
          continue;
        }
        // A workspace sibling is declared in at least one package; it is not an
        // unlisted external dependency.
        if (allNames.has(name)) continue;
        // A package provided by a declared dependency (peer, optional, or a
        // known runtime requirement) is informational. Removing it would be
        // redundant, not a missing declaration.
        const target = declaredRequirements.has(name) ? transitiveByName : byName;
        const entry = target.get(name) ?? { importedBy: [], specifiers: [] };
        if (!entry.importedBy.includes(module.path)) entry.importedBy.push(module.path);
        if (!entry.specifiers.includes(specifier)) entry.specifiers.push(specifier);
        target.set(name, entry);
      }
    }
    const unlisted: UnlistedDependency[] = [];
    for (const [name, entry] of [...byName].sort(([a], [b]) => a.localeCompare(b))) {
      unlisted.push({
        name,
        importedBy: entry.importedBy.sort(),
        specifiers: entry.specifiers.sort(),
      });
    }
    const transitive: UnlistedDependency[] = [];
    for (const [name, entry] of [...transitiveByName].sort(([a], [b]) => a.localeCompare(b))) {
      transitive.push({
        name,
        importedBy: entry.importedBy.sort(),
        specifiers: entry.specifiers.sort(),
      });
    }

    if (unused.length === 0 && unlisted.length === 0 && transitive.length === 0) continue;
    reports.push({
      name: context.name,
      directory: context.directory,
      unused,
      unlisted,
      transitive,
      counts: {
        dependencies: Object.keys(context.json.dependencies ?? {}).length,
        devDependencies: Object.keys(context.json.devDependencies ?? {}).length,
        used: used.size,
      },
    });
  }

  const unusedByKind: Record<DependencyKind, number> = {
    dependencies: 0,
    devDependencies: 0,
    optionalDependencies: 0,
    peerDependencies: 0,
  };
  for (const report of reports) {
    for (const item of report.unused) unusedByKind[item.kind] += 1;
  }
  return {
    packages: reports,
    summary: {
      packages: reports.length,
      unused: reports.reduce((total, report) => total + report.unused.length, 0),
      unlisted: reports.reduce((total, report) => total + report.unlisted.length, 0),
      transitive: reports.reduce((total, report) => total + report.transitive.length, 0),
      unusedByKind,
    },
  };
}
