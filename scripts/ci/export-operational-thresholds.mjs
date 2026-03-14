#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const configPath = resolve(".codex/config/operational-gates.json");
const config = JSON.parse(readFileSync(configPath, "utf8"));
const thresholds = config?.thresholds;
const shellMode = process.argv.includes("--shell");

if (!thresholds) {
  console.error("missing thresholds in .codex/config/operational-gates.json");
  process.exit(1);
}

const output = {
  PERF_BUNDLE_DELTA_MAX_RATIO: thresholds.perf?.bundle_regression_ratio_max,
  PERF_BUILD_DELTA_MAX_RATIO: thresholds.perf?.build_regression_ratio_max,
  PERF_ASSET_MAX_BYTES: thresholds.perf?.asset_max_bytes,
  PERF_API_P95_MS_MAX: thresholds.perf?.api_p95_ms_max,
  PERF_API_P99_MS_MAX: thresholds.perf?.api_p99_ms_max,
  RELIABILITY_SOAK_MINUTES_MIN: thresholds.reliability?.soak_minutes_min,
  RELIABILITY_RECONNECT_STORM_ERROR_RATE_MAX: thresholds.reliability?.reconnect_storm_error_rate_max,
  RELIABILITY_RELAY_QUEUE_DROP_RATE_MAX: thresholds.reliability?.relay_queue_drop_rate_max,
  SECURITY_MAX_CRITICAL_VULNS: thresholds.security?.max_critical_vulnerabilities,
  SECURITY_MAX_UNRESOLVED_HIGH_VULNS: thresholds.security?.max_unresolved_high_vulnerabilities,
  SECURITY_REQUIRE_SIGNED_ARTIFACTS: thresholds.security?.require_signed_artifacts,
  SECURITY_REQUIRE_SBOM: thresholds.security?.require_sbom,
  SECURITY_REQUIRE_PROVENANCE: thresholds.security?.require_provenance,
};

for (const [key, value] of Object.entries(output)) {
  const validValue =
    (typeof value === "number" && !Number.isNaN(value)) ||
    typeof value === "boolean";
  if (!validValue) {
    console.error(`invalid threshold for ${key}`);
    process.exit(1);
  }

  if (shellMode) {
    // Shell-export format for inline eval in local verification.
    const rendered = typeof value === "boolean" ? (value ? "true" : "false") : `${value}`;
    console.log(`export ${key}=${rendered}`);
  } else {
    // Printed in KEY=VALUE format for $GITHUB_ENV consumption.
    console.log(`${key}=${value}`);
  }
}
