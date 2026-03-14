#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const CONFIG_PATH = resolve(".codex/config/operational-gates.json");
const RELEASE_CHANNELS_PATH = resolve("client/src-tauri/release-channels.json");
const RELEASE_MATERIALS_PATH = resolve(".codex/config/release-materials.required.json");

function fail(message) {
  console.error(`config validation failed: ${message}`);
  process.exit(1);
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    fail(`could not read JSON at ${path}: ${error.message}`);
  }
}

function assert(condition, message) {
  if (!condition) {
    fail(message);
  }
}

function assertNumberInRange(value, name, min = 0, max = Number.POSITIVE_INFINITY) {
  assert(typeof value === "number" && Number.isFinite(value), `${name} must be a finite number`);
  assert(value >= min, `${name} must be >= ${min}`);
  assert(value <= max, `${name} must be <= ${max}`);
}

const config = readJson(CONFIG_PATH);
assert(config.schema_version === 1, "schema_version must be 1");

assert(typeof config.protocol?.current_version === "string", "protocol.current_version missing");
assert(
  config.protocol?.compatibility_policy === "N-1 minor (same major)",
  "protocol.compatibility_policy must be 'N-1 minor (same major)'"
);

const channels = config.release_channels?.channels;
assert(config.release_channels?.default === "stable", "release_channels.default must be stable");
assert(channels && typeof channels === "object", "release_channels.channels missing");
for (const channel of ["internal", "beta", "stable"]) {
  assert(channels[channel], `release channel '${channel}' missing`);
  assertNumberInRange(channels[channel].ring, `${channel}.ring`, 0, 9);
}

const thresholds = config.thresholds;
assert(thresholds && typeof thresholds === "object", "thresholds missing");
const apiP95MetricKey = ["api", "p95", "ms", "max"].join("_");
const apiP99MetricKey = ["api", "p99", "ms", "max"].join("_");
assertNumberInRange(thresholds.perf?.bundle_regression_ratio_max, "thresholds.perf.bundle_regression_ratio_max", 0, 1);
assertNumberInRange(thresholds.perf?.build_regression_ratio_max, "thresholds.perf.build_regression_ratio_max", 0, 1);
assertNumberInRange(thresholds.perf?.asset_max_bytes, "thresholds.perf.asset_max_bytes", 1);
assertNumberInRange(thresholds.perf?.api_p95_ms_max, `thresholds.perf.${apiP95MetricKey}`, 1);
assertNumberInRange(thresholds.perf?.api_p99_ms_max, `thresholds.perf.${apiP99MetricKey}`, 1);
assertNumberInRange(thresholds.reliability?.soak_minutes_min, "thresholds.reliability.soak_minutes_min", 1);
assertNumberInRange(
  thresholds.reliability?.reconnect_storm_error_rate_max,
  "thresholds.reliability.reconnect_storm_error_rate_max",
  0,
  1
);
assertNumberInRange(
  thresholds.reliability?.relay_queue_drop_rate_max,
  "thresholds.reliability.relay_queue_drop_rate_max",
  0,
  1
);
assertNumberInRange(
  thresholds.security?.max_critical_vulnerabilities,
  "thresholds.security.max_critical_vulnerabilities",
  0
);
assertNumberInRange(
  thresholds.security?.max_unresolved_high_vulnerabilities,
  "thresholds.security.max_unresolved_high_vulnerabilities",
  0
);
assert(typeof thresholds.security?.require_signed_artifacts === "boolean", "thresholds.security.require_signed_artifacts must be boolean");
assert(typeof thresholds.security?.require_sbom === "boolean", "thresholds.security.require_sbom must be boolean");
assert(typeof thresholds.security?.require_provenance === "boolean", "thresholds.security.require_provenance must be boolean");

const releaseChannels = readJson(RELEASE_CHANNELS_PATH);
assert(releaseChannels.schema_version === 1, "release channels schema_version must be 1");
assert(releaseChannels.default_channel === "stable", "release channels default_channel must be stable");
for (const channel of ["internal", "beta", "stable"]) {
  const row = releaseChannels.channels?.[channel];
  assert(row, `release-channels.json missing '${channel}'`);
  assertNumberInRange(row.ring, `release-channels.${channel}.ring`, 0, 9);
  assertNumberInRange(
    row.update_window_hours,
    `release-channels.${channel}.update_window_hours`,
    1,
    24 * 30
  );
}

const releaseMaterials = readJson(RELEASE_MATERIALS_PATH);
assert(releaseMaterials.schema_version === 1, "release materials schema_version must be 1");
assert(Array.isArray(releaseMaterials.required_env), "release materials required_env must be an array");
assert(releaseMaterials.required_env.length > 0, "release materials required_env must not be empty");
for (const item of releaseMaterials.required_env) {
  assert(typeof item === "string" && item.length > 0, "release materials env names must be non-empty strings");
}

console.log("operational configuration validated");
