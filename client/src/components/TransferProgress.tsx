import { Show } from "solid-js";
import { transfer, resetTransfer } from "../stores/transfer";
import { cancelTransfer } from "../lib/tauri-bridge";
import { formatBytes, formatSpeed, formatETA } from "../lib/format";

export default function TransferProgress() {
  async function handleCancel() {
    try {
      await cancelTransfer(transfer.sessionId);
    } catch {
      // ignore
    }
    resetTransfer();
  }

  return (
    <div class="w-full max-w-md space-y-6">
      <h2 class="text-xl font-semibold text-center">
        {transfer.role === "sender" ? "Sending" : "Receiving"}...
      </h2>

      {/* Overall progress bar */}
      <div class="space-y-2">
        <div class="flex justify-between text-sm text-[#a0a0a0]">
          <span>
            {formatBytes(transfer.progress.bytesTransferred)} /{" "}
            {formatBytes(transfer.progress.bytesTotal)}
          </span>
          <span>{transfer.progress.percent.toFixed(1)}%</span>
        </div>
        <div class="w-full h-3 bg-[#1e1e1e] rounded-full overflow-hidden">
          <div
            class="h-full bg-[#3b82f6] rounded-full transition-all duration-300 ease-out"
            style={{ width: `${Math.min(transfer.progress.percent, 100)}%` }}
          />
        </div>
      </div>

      {/* Stats */}
      <div class="grid grid-cols-2 gap-4">
        <div class="bg-[#141414] rounded-lg p-3">
          <p class="text-xs text-[#a0a0a0] uppercase">Speed</p>
          <p class="text-lg font-semibold">
            {formatSpeed(transfer.progress.speedBps)}
          </p>
        </div>
        <div class="bg-[#141414] rounded-lg p-3">
          <p class="text-xs text-[#a0a0a0] uppercase">ETA</p>
          <p class="text-lg font-semibold">
            {formatETA(transfer.progress.etaSeconds)}
          </p>
        </div>
      </div>

      {/* Current file */}
      <Show when={transfer.progress.currentFile}>
        <div class="text-center text-sm text-[#a0a0a0]">
          Transferring: {transfer.progress.currentFile}
        </div>
      </Show>

      {/* Completed files */}
      <Show when={transfer.progress.completedFiles.length > 0}>
        <div class="text-xs text-[#a0a0a0] space-y-1">
          {transfer.progress.completedFiles.map((name) => (
            <div class="flex items-center gap-1">
              <span class="text-[#22c55e]">{"\u2713"}</span> {name}
            </div>
          ))}
        </div>
      </Show>

      <div class="text-center">
        <button
          class="px-6 py-2 text-sm text-[#a0a0a0] hover:text-red-400 border border-[#333] hover:border-red-400/50 rounded-lg transition-colors"
          onClick={handleCancel}
        >
          Cancel Transfer
        </button>
      </div>
    </div>
  );
}
