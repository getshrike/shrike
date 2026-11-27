#!/usr/bin/env node
/**
 * Skill gate: validate the public skill source, its references, its relative
 * links, and that `packages/cli/dist/skills/` is byte-identical when present.
 *
 * The validator includes a self-test so a validator that always passes cannot
 * ship. Run `node scripts/check-skills.mjs --self-test` to run only the
 * self-test.
 */
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const skillsRoot = join(root, "skills");
const distSkillsRoot = join(root, "packages", "cli", "dist", "skills");
const COMMANDS = new Set([
  "init", "report", "scan", "deps", "delta", "plan", "clean", "prune", "why",
  "verify", "receipt", "recover", "proof", "evidence", "install", "ci", "hook",
  "mcp", "cache", "version",
]);
const NAME_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const ABSOLUTE_PATHS = [
  /\/home\//, /\/Users\//, /\/tmp\//, /\/var\//, /\/etc\//,
  /(?:^|[\s"'`(<])[A-Za-z]:[\\/]/m,
];

function listFiles(directory) {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...listFiles(path));
    else files.push(path);
  }
  return files;
}

function relativeFiles(directory) {
  return listFiles(directory)
    .map((path) => relative(directory, path).split(sep).join("/"))
    .sort();
}

function read(path) {
  return readFileSync(path, "utf8");
}

function frontmatter(text) {
  if (!text.startsWith("---\n")) return { error: "frontmatter must start with --- followed by a newline" };
  const end = text.indexOf("\n---\n", 4);
  if (end < 0) return { error: "frontmatter is not terminated by ---" };
  const fields = {};
  for (const line of text.slice(4, end).split("\n")) {
    if (line.trim().length === 0) continue;
    const match = /^([A-Za-z0-9_-]+):[ \t]*(.*)$/.exec(line);
    if (!match) return { error: `frontmatter line is not key: value: ${line}` };
    fields[match[1]] = match[2];
  }
  return { fields, body: text.slice(end + 5) };
}

function stripFences(text) {
  return text.replace(/^```[\s\S]*?^```/gm, "");
}

function links(text) {
  const found = [];
  for (const match of stripFences(text).matchAll(/\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)) {
    found.push(match[1]);
  }
  return found;
}

function inside(directory, path) {
  const rel = relative(directory, path);
  return rel === "" || (!rel.startsWith(`..${sep}`) && rel !== ".." && !isAbsolute(rel));
}

function validateLinks(directory, relPath, text, failures) {
  for (const target of links(text)) {
    if (target.startsWith("#")) continue;
    if (/^https:/i.test(target)) continue;
    if (/^[a-z][a-z0-9+.-]*:/i.test(target)) {
      failures.push(`${relPath}: link ${target} must be relative or https`);
      continue;
    }
    const pathPart = target.split("#")[0] ?? "";
    if (pathPart.length === 0) continue;
    const resolved = resolve(dirname(join(directory, relPath)), pathPart);
    if (!inside(directory, resolved)) {
      failures.push(`${relPath}: link ${target} escapes the skill directory`);
      continue;
    }
    if (!existsSync(resolved) || !statSync(resolved).isFile()) {
      failures.push(`${relPath}: link ${target} does not resolve`);
    }
  }
}

function validateCommands(relPath, text, failures) {
  for (const match of text.matchAll(/\b(?:getshrike|shrike)[ \t]+([a-z][a-z0-9-]*)/g)) {
    const command = match[1];
    if (!COMMANDS.has(command)) {
      failures.push(`${relPath}: unknown command ${JSON.stringify(command)} in ${JSON.stringify(match[0])}`);
    }
  }
}

/** Validate one skill directory. Returns a list of human-readable failures. */
export function validateSkillDirectory(
  directory,
  expectedName = relative(skillsRoot, directory).split(sep).join("/"),
) {
  const failures = [];
  const name = expectedName;
  const mainPath = join(directory, "SKILL.md");
  if (!existsSync(mainPath) || !statSync(mainPath).isFile()) {
    return [`${name}: SKILL.md is missing`];
  }

  const files = listFiles(directory);
  for (const path of files) {
    if (!path.endsWith(".md")) {
      failures.push(`${name}: shipped skill contains a non-Markdown file: ${relative(directory, path).split(sep).join("/")}`);
    }
  }

  const mainText = read(mainPath);
  const parsed = frontmatter(mainText);
  if (parsed.error) {
    failures.push(`${name}/SKILL.md: ${parsed.error}`);
    return failures;
  }
  const { fields, body } = parsed;

  for (const key of ["name", "description", "license"]) {
    if (typeof fields[key] !== "string" || fields[key].trim().length === 0) {
      failures.push(`${name}/SKILL.md: frontmatter is missing ${key}`);
    }
  }
  if (typeof fields.name === "string") {
    if (fields.name.length > 64) failures.push(`${name}/SKILL.md: name is longer than 64 characters`);
    if (!NAME_RE.test(fields.name)) failures.push(`${name}/SKILL.md: name does not match ${NAME_RE}`);
    if (fields.name !== name) failures.push(`${name}/SKILL.md: name ${JSON.stringify(fields.name)} does not match directory ${JSON.stringify(name)}`);
  }
  if (typeof fields.description === "string") {
    const description = fields.description;
    if (description.length > 1024) failures.push(`${name}/SKILL.md: description is longer than 1024 characters`);
    if (!/Shrike|getshrike/.test(description)) failures.push(`${name}/SKILL.md: description does not name Shrike or getshrike`);
    if (!/Use when/.test(description)) failures.push(`${name}/SKILL.md: description is missing "Use when"`);
    const signals = ["dead code", "unused", "cleanup", "orphan", "receipt", "CI", "PR"];
    if (signals.filter((signal) => description.includes(signal)).length < 2) {
      failures.push(`${name}/SKILL.md: description needs at least two trigger signals`);
    }
  }

  const bodyLines = body.replace(/\n$/, "").split("\n");
  if (bodyLines.length > 300) failures.push(`${name}/SKILL.md: body is ${bodyLines.length} lines, above 300`);
  const bodyBytes = Buffer.byteLength(body, "utf8");
  if (bodyBytes > 24 * 1024) failures.push(`${name}/SKILL.md: body is ${bodyBytes} bytes, above 24 KB`);
  if (!/^# [^\n]*\n/.test(body.trimStart())) {
    failures.push(`${name}/SKILL.md: body must start with one H1`);
  } else if ((body.match(/^# /gm) ?? []).length !== 1) {
    failures.push(`${name}/SKILL.md: body must contain exactly one H1`);
  }

  const referencesDir = join(directory, "references");
  const references = [];
  if (existsSync(referencesDir)) {
    if (!statSync(referencesDir).isDirectory()) {
      failures.push(`${name}: references must be a directory`);
    } else {
      for (const entry of readdirSync(referencesDir, { withFileTypes: true })) {
        if (entry.isDirectory()) {
          failures.push(`${name}: nested reference directory ${entry.name} is not allowed`);
          continue;
        }
        if (!entry.name.endsWith(".md")) {
          failures.push(`${name}: reference ${entry.name} must be Markdown`);
          continue;
        }
        references.push(join(referencesDir, entry.name));
      }
    }
  }

  const mainLinks = links(mainText)
    .filter((target) => !/^[a-z][a-z0-9+.-]*:/i.test(target) && !target.startsWith("#"))
    .map((target) => resolve(directory, target.split("#")[0] ?? ""));
  for (const reference of references) {
    const relPath = relative(directory, reference).split(sep).join("/");
    if (!mainLinks.includes(resolve(reference))) {
      failures.push(`${name}/SKILL.md: does not link reference ${relPath}`);
    }
    const referenceText = read(reference);
    const referenceLines = referenceText.replace(/\n$/, "").split("\n").length;
    if (referenceLines > 400) {
      failures.push(`${relPath}: reference is ${referenceLines} lines, above 400`);
    }
    const referenceBytes = Buffer.byteLength(referenceText, "utf8");
    if (referenceBytes > 32 * 1024) {
      failures.push(`${relPath}: reference is ${referenceBytes} bytes, above 32 KB`);
    }
  }

  for (const path of files) {
    const relPath = relative(directory, path).split(sep).join("/");
    const text = read(path);
    if (text.includes("\r\n")) failures.push(`${relPath}: contains CRLF line endings`);
    for (const pattern of ABSOLUTE_PATHS) {
      if (pattern.test(text)) failures.push(`${relPath}: contains an absolute host path`);
    }
    validateLinks(directory, relPath, text, failures);
    validateCommands(relPath, text, failures);
    if (/\bnpx\s+shrike\b/.test(text)) {
      failures.push(`${relPath}: contains the forbidden zero-install form; use npx getshrike`);
    }
    for (const match of text.matchAll(/--no-checks/g)) {
      const start = Math.max(0, (match.index ?? 0) - 200);
      const window = text.slice(start, (match.index ?? 0) + 200);
      if (!/reject|not supported|never|refus/i.test(window)) {
        failures.push(`${relPath}: --no-checks must be described as rejected`);
      }
    }
    if (path !== mainPath && statSync(path).isDirectory()) {
      failures.push(`${relPath}: unexpected directory`);
    }
  }

  return failures;
}

/** Compare the authored tree with a built copy, when the copy exists. */
export function distParityFailures(source, dist) {
  if (!existsSync(dist)) return [];
  const failures = [];
  const sourceFiles = relativeFiles(source);
  const distFiles = relativeFiles(dist);
  const sourceSet = new Set(sourceFiles);
  const distSet = new Set(distFiles);
  for (const path of sourceFiles) {
    if (!distSet.has(path)) {
      failures.push(`dist/skills/${path}: missing from the build`);
      continue;
    }
    if (!readFileSync(join(source, path)).equals(readFileSync(join(dist, path)))) {
      failures.push(`dist/skills/${path}: differs from skills/${path}`);
    }
  }
  for (const path of distFiles) {
    if (!sourceSet.has(path)) failures.push(`dist/skills/${path}: unexpected built file`);
  }
  return failures;
}

function discover() {
  if (!existsSync(skillsRoot) || !statSync(skillsRoot).isDirectory()) return [];
  return readdirSync(skillsRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join(skillsRoot, entry.name))
    .sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
}

function discoveryFailures(directory = skillsRoot) {
  if (!existsSync(directory)) return ["skills/: no public skill directories found"];
  if (!statSync(directory).isDirectory()) return ["skills/: expected a directory"];
  const failures = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isDirectory()) {
      failures.push(`skills/${entry.name}: direct child must be a skill directory`);
    }
  }
  return failures;
}

function selfTest() {
  const failures = [];
  const real = join(skillsRoot, "shrike");
  if (!existsSync(real)) return ["self-test: skills/shrike is missing"];
  const realFailures = validateSkillDirectory(real, "shrike");
  if (realFailures.length > 0) {
    return [`self-test: the real skill does not validate: ${realFailures[0]}`];
  }

  const temp = mkdtempSync(join(tmpdir(), "shrike-skill-selftest-"));
  const cases = [];
  const mutate = (name, change) => {
    const directory = join(temp, name);
    cpSync(real, join(directory, "shrike"), { recursive: true });
    change(join(directory, "shrike"));
    cases.push({ name, directory: join(directory, "shrike") });
  };
  try {
    mutate("no-frontmatter", (directory) => writeFileSync(join(directory, "SKILL.md"), "# no frontmatter\n", "utf8"));
    mutate("name-mismatch", (directory) => {
      const path = join(directory, "SKILL.md");
      writeFileSync(path, read(path).replace("name: shrike", "name: other"), "utf8");
    });
    mutate("long-description", (directory) => {
      const path = join(directory, "SKILL.md");
      writeFileSync(path, read(path).replace(/^description: .*$/m, `description: ${"x".repeat(1100)}`), "utf8");
    });
    mutate("missing-reference", (directory) => rmSync(join(directory, "references", "commands.md")));
    mutate("absolute-path", (directory) => {
      const path = join(directory, "SKILL.md");
      writeFileSync(path, `${read(path)}\nSee /home/private/project.\n`, "utf8");
    });
    mutate("windows-path", (directory) => {
      const path = join(directory, "SKILL.md");
      writeFileSync(path, `${read(path)}\nSee \`C:\\Users\\private\\project\`.\n`, "utf8");
    });
    mutate("crlf", (directory) => {
      const path = join(directory, "SKILL.md");
      writeFileSync(path, read(path).replaceAll("\n", "\r\n"), "utf8");
    });
    mutate("unknown-command", (directory) => {
      const path = join(directory, "SKILL.md");
      writeFileSync(path, `${read(path)}\nRun \`shrike frobnicate\`.\n`, "utf8");
    });
    mutate("npx-shrike", (directory) => {
      const path = join(directory, "SKILL.md");
      writeFileSync(path, `${read(path)}\nRun \`npx shrike report\`.\n`, "utf8");
    });
    mutate("overlong-reference", (directory) => {
      const path = join(directory, "references", "commands.md");
      writeFileSync(path, `${read(path)}\n${"x\n".repeat(401)}`, "utf8");
    });

    for (const testCase of cases) {
      if (validateSkillDirectory(testCase.directory, "shrike").length === 0) {
        failures.push(`self-test: validator accepted ${testCase.name}`);
      }
    }

    const distCase = join(temp, "dist-mismatch");
    mkdirSync(join(distCase, "skills", "shrike"), { recursive: true });
    cpSync(real, join(distCase, "skills", "shrike"), { recursive: true });
    writeFileSync(join(distCase, "skills", "shrike", "SKILL.md"), "different\n", "utf8");
    if (distParityFailures(real, join(distCase, "skills", "shrike")).length === 0) {
      failures.push("self-test: parity check accepted a different dist file");
    }
    const emptyDist = join(temp, "dist-empty");
    mkdirSync(emptyDist);
    if (distParityFailures(real, emptyDist).length === 0) {
      failures.push("self-test: parity check accepted an empty dist tree");
    }
    const strayRoot = join(temp, "stray-root");
    mkdirSync(strayRoot);
    writeFileSync(join(strayRoot, "README.md"), "# not a skill\n", "utf8");
    if (discoveryFailures(strayRoot).length === 0) {
      failures.push("self-test: discovery accepted a non-directory child");
    }
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
  return failures;
}

const selfTestOnly = process.argv.includes("--self-test");
const failures = [];
if (!selfTestOnly) {
  failures.push(...discoveryFailures());
  const skills = discover();
  if (skills.length === 0) failures.push("skills/: no public skill directories found");
  if (!skills.some((directory) => relative(skillsRoot, directory) === "shrike")) {
    failures.push("skills/shrike: the public Shrike skill is missing");
  }
  for (const directory of skills) {
    failures.push(...validateSkillDirectory(directory));
  }
  if (existsSync(distSkillsRoot)) {
    if (!statSync(distSkillsRoot).isDirectory()) {
      failures.push("packages/cli/dist/skills: expected a directory");
    } else {
      failures.push(...distParityFailures(skillsRoot, distSkillsRoot));
    }
  }
}
failures.push(...selfTest());

if (failures.length > 0) {
  for (const failure of failures) console.error(`FAIL skill: ${failure}`);
  process.exit(1);
}
const skills = discover();
const referenceCount = skills.reduce((count, directory) => {
  const references = join(directory, "references");
  return count + (existsSync(references) ? readdirSync(references).length : 0);
}, 0);
console.log(
  `OK: ${skills.length} public skill (${referenceCount} references), links, commands, budgets, LF, and dist parity; ` +
    "validator self-test rejected malformed frontmatter, links, paths, commands, discovery, and dist drift",
);
