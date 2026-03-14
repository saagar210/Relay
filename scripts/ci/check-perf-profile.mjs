#!/usr/bin/env node

const mode = process.argv[2] || "foundation";

const profile = process.env.PERF_PROFILE || "";
const baseUrl = process.env.PERF_BASE_URL || "";
const databaseUrl = process.env.PERF_DATABASE_URL || "";

function fail(message) {
  console.error(`perf profile check failed: ${message}`);
  process.exit(1);
}

if (mode === "enforced") {
  if (profile !== "production") {
    fail("PERF_PROFILE must be set to 'production' for perf-enforced workflow");
  }
  if (!baseUrl) {
    fail("PERF_BASE_URL must be set for perf-enforced workflow");
  }
  if (!databaseUrl) {
    fail("PERF_DATABASE_URL must be set for perf-enforced workflow");
  }
}

if (mode === "foundation") {
  if (!profile) {
    console.log("PERF_PROFILE is unset in foundation mode (allowed)");
  }
  if (!baseUrl || !databaseUrl) {
    console.log("PERF_BASE_URL or PERF_DATABASE_URL unset in foundation mode (external probes skipped)");
  }
}

console.log(`perf profile check passed (${mode})`);
