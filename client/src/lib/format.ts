/** Format bytes as human-readable string (e.g., "12.4 MB") */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const value = bytes / Math.pow(k, i);
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[i]}`;
}

/** Format bytes/sec as human-readable speed (e.g., "12.4 MB/s") */
export function formatSpeed(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/s`;
}

/** Format seconds as human-readable ETA (e.g., "2 min remaining") */
export function formatETA(seconds: number): string {
  if (seconds <= 0) return "calculating...";
  if (seconds < 60) return `${seconds}s remaining`;
  if (seconds < 3600) {
    const mins = Math.ceil(seconds / 60);
    return `${mins} min remaining`;
  }
  const hours = Math.floor(seconds / 3600);
  const mins = Math.ceil((seconds % 3600) / 60);
  return `${hours}h ${mins}m remaining`;
}
