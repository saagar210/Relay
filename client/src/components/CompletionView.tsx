import { transfer, resetTransfer, setTransfer } from "../stores/transfer";
import { formatBytes, formatSpeed } from "../lib/format";

export default function CompletionView() {
  return (
    <div class="text-center space-y-6 max-w-md">
      <div class="w-16 h-16 mx-auto rounded-full bg-[#22c55e]/20 flex items-center justify-center">
        <span class="text-[#22c55e] text-3xl">{"\u2713"}</span>
      </div>

      <div class="space-y-1">
        <h2 class="text-2xl font-semibold">Transfer Complete</h2>
        <p class="text-[#a0a0a0]">
          {transfer.summary.fileCount} file(s) transferred successfully
        </p>
      </div>

      <div class="grid grid-cols-3 gap-4 text-center">
        <div class="bg-[#141414] rounded-lg p-3">
          <p class="text-xs text-[#a0a0a0] uppercase">Total</p>
          <p class="font-semibold">{formatBytes(transfer.summary.totalBytes)}</p>
        </div>
        <div class="bg-[#141414] rounded-lg p-3">
          <p class="text-xs text-[#a0a0a0] uppercase">Duration</p>
          <p class="font-semibold">{transfer.summary.durationSeconds}s</p>
        </div>
        <div class="bg-[#141414] rounded-lg p-3">
          <p class="text-xs text-[#a0a0a0] uppercase">Avg Speed</p>
          <p class="font-semibold">
            {formatSpeed(transfer.summary.averageSpeed)}
          </p>
        </div>
      </div>

      <div class="flex gap-3 justify-center">
        <button
          class="px-6 py-3 bg-[#3b82f6] hover:bg-[#2563eb] rounded-lg font-semibold transition-colors"
          onClick={() => {
            resetTransfer();
            setTransfer("phase", "selecting");
            setTransfer("role", "sender");
          }}
        >
          Send More
        </button>
        <button
          class="px-6 py-3 bg-[#1e1e1e] hover:bg-[#2a2a2a] border border-[#333] rounded-lg transition-colors"
          onClick={() => {
            resetTransfer();
            setTransfer("phase", "entering-code");
            setTransfer("role", "receiver");
          }}
        >
          Receive More
        </button>
      </div>

      <button
        class="text-sm text-[#a0a0a0] hover:text-white transition-colors"
        onClick={resetTransfer}
      >
        Done
      </button>
    </div>
  );
}
