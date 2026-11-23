#!/usr/bin/env node
/** Repository check: the thin AGENTS.md, development docs, and .agent guidance exist. */
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const failures = [];
const docs = ["README.md", "development.md", "features.md", "how-it-works.md", "architecture.md", "testing.md", "contributing.md", "release.md"];

for (const name of docs) {
  const path = join(root, "docs", name);
  if (!existsSync(path) || readFileSync(path, "utf8").trim().length < 100) {
    failures.push(`docs/${name} is missing or empty`);
  }
}

const agentsPath = join(root, "AGENTS.md");
if (!existsSync(agentsPath)) {
  failures.push("AGENTS.md is missing");
} else {
  const agents = readFileSync(agentsPath, "utf8");
  if (agents.length > 6000) failures.push("AGENTS.md is too long; keep it thin and link to docs");
  for (const name of docs) {
    if (!agents.includes(`docs/${name}`)) failures.push(`AGENTS.md does not link docs/${name}`);
  }
}

const rulesDir = join(root, ".agent", "rules");
const rules = existsSync(rulesDir) ? readdirSync(rulesDir).filter((name) => name.endsWith(".md")) : [];
if (rules.length < 3) failures.push(".agent/rules: expected at least 3 rule files");

const skillsDir = join(root, ".agent", "skills");
const skills = existsSync(skillsDir)
  ? readdirSync(skillsDir, { withFileTypes: true }).filter((entry) => entry.isDirectory()).map((entry) => entry.name)
  : [];
if (skills.length < 3) failures.push(".agent/skills: expected at least 3 skills");
for (const name of skills) {
  const path = join(skillsDir, name, "SKILL.md");
  if (!existsSync(path)) {
    failures.push(`.agent/skills/${name}/SKILL.md is missing`);
    continue;
  }
  const body = readFileSync(path, "utf8");
  if (!body.startsWith("---\n") || !body.includes("name:") || !body.includes("description:")) {
    failures.push(`.agent/skills/${name}/SKILL.md is missing frontmatter`);
  }
}

if (failures.length) {
  for (const failure of failures) console.error(`FAIL ${failure}`);
  process.exit(1);
}
console.log(`OK: ${docs.length} docs linked from a thin AGENTS.md, ${rules.length} rules, ${skills.length} skills`);
