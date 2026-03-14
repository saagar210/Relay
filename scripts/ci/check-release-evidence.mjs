#!/usr/bin/env node

import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const enforce = process.argv.includes("--enforce") || process.env.RELAY_ENFORCE_RELEASE_EVIDENCE === "1";
const gateFlagIndex = process.argv.indexOf("--gate");
const gate = gateFlagIndex >= 0 ? process.argv[gateFlagIndex + 1] : "all";
const eventName = process.env.GITHUB_EVENT_NAME || "";
const shouldEnforce = enforce || eventName === "release";

const requiredDocs = [
  "docs/release/README.md",
  "docs/release/RELEASE_GATE_OWNERSHIP.md",
  "docs/release/RELEASE_CHANNELS.md",
  "docs/release/COMPATIBILITY_POLICY.md",
  "docs/release/ROLLBACK_PLAYBOOK.md",
  "docs/release/CANARY_SCORECARD.md",
];

const requiredTemplates = [
  ".codex/evidence/README.md",
  ".codex/evidence/g3/README.md",
  ".codex/evidence/g5/README.md",
];

const requiredG3Evidence = [
  ".codex/evidence/g3/sbom.spdx.json",
  ".codex/evidence/g3/provenance.intoto.jsonl",
  ".codex/evidence/g3/artifact-signature.txt",
];

const requiredG5Evidence = [
  ".codex/evidence/g5/canary-result.json",
  ".codex/evidence/g5/rollback-drill.json",
];

function fail(message) {
  console.error(`release evidence check failed: ${message}`);
  process.exit(1);
}

function assertExists(path) {
  if (!existsSync(resolve(path))) {
    fail(`missing required file: ${path}`);
  }
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(resolve(path), "utf8"));
  } catch (error) {
    fail(`invalid JSON at ${path}: ${error.message}`);
  }
}

for (const doc of requiredDocs) {
  assertExists(doc);
}

for (const template of requiredTemplates) {
  assertExists(template);
}

if (!shouldEnforce) {
  console.log(`release evidence templates and governance docs are present (gate=${gate})`);
  process.exit(0);
}

const policy = readJson(".codex/config/operational-gates.json");
const security = policy?.thresholds?.security || {};

if (
  (gate === "all" || gate === "g3") &&
  (security.require_sbom === true || security.require_provenance === true || security.require_signed_artifacts === true)
) {
  for (const file of requiredG3Evidence) {
    assertExists(file);
  }
}

if (gate === "all" || gate === "g5") {
  for (const file of requiredG5Evidence) {
    assertExists(file);
  }
}

if (gate === "all" || gate === "g5") {
  const canary = readJson(".codex/evidence/g5/canary-result.json");
  if (!["promote", "hold", "rollback"].includes(canary?.result)) {
    fail("canary-result.json must set result to promote|hold|rollback");
  }
  if (canary?.approved !== true) {
    fail("canary-result.json must set approved=true for enforced promotion");
  }

  const rollback = readJson(".codex/evidence/g5/rollback-drill.json");
  if (rollback?.passed !== true) {
    fail("rollback-drill.json must set passed=true for enforced promotion");
  }
  if (typeof rollback?.rollback_minutes !== "number" || rollback.rollback_minutes <= 0) {
    fail("rollback-drill.json must include rollback_minutes > 0");
  }
}
console.log(`release evidence checks passed (gate=${gate})`);
