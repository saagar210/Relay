#!/usr/bin/env node

import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const enforce = process.argv.includes("--enforce") || process.env.RELAY_ENFORCE_RELEASE_MATERIALS === "1";
if (!enforce) {
  console.log("release materials preflight is in advisory mode");
  process.exit(0);
}

const configPath = resolve(".codex/config/release-materials.required.json");
if (!existsSync(configPath)) {
  console.error("release materials check failed: missing .codex/config/release-materials.required.json");
  process.exit(1);
}

const config = JSON.parse(readFileSync(configPath, "utf8"));
const requiredEnv = Array.isArray(config?.required_env) ? config.required_env : [];
if (requiredEnv.length === 0) {
  console.error("release materials check failed: required_env is empty");
  process.exit(1);
}

const missing = requiredEnv.filter((name) => !process.env[name] || `${process.env[name]}`.trim() === "");
if (missing.length > 0) {
  console.error(`release materials check failed: missing required env: ${missing.join(", ")}`);
  process.exit(1);
}

console.log("release materials preflight passed");
