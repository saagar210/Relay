import { createSignal, Show } from "solid-js";
import { transfer, setTransfer, resetTransfer } from "../stores/transfer";
import { startReceive, acceptTransfer } from "../lib/tauri-bridge";
import { formatBytes } from "../lib/format";
import CodeInput from "./CodeInput";
import FileList from "./FileList";

export default function ReceiveView() {
  const [senderAddr, setSenderAddr] = createSignal("");
  const [saveDir, setSaveDir] = createSignal("");

  async function handleCodeSubmit(code: string) {
    if (!senderAddr()) {
      // For Phase 1 LAN demo, need sender address
      return;
    }
    setTransfer("phase", "connecting");
    try {
      const dir = saveDir() || "/tmp/relay-received";
      const sessionId = await startReceive(code, dir, senderAddr());
      setTransfer("sessionId", sessionId);
    } catch (e) {
      setTransfer("phase", "error");
      setTransfer("error", String(e));
    }
  }

  async function handleAccept(accept: boolean) {
    try {
      await acceptTransfer(transfer.sessionId, accept);
      if (!accept) resetTransfer();
    } catch (e) {
      setTransfer("phase", "error");
      setTransfer("error", String(e));
    }
  }

  return (
    <div class="w-full max-w-lg space-y-6">
      <Show when={transfer.phase === "entering-code"}>
        <div class="space-y-6">
          <h2 class="text-2xl font-semibold text-center">Receive Files</h2>
          <CodeInput onSubmit={handleCodeSubmit} />

          {/* Phase 1: Manual sender address input */}
          <div class="space-y-3 pt-2">
            <input
              type="text"
              value={senderAddr()}
              onInput={(e) => setSenderAddr(e.currentTarget.value)}
              class="w-full px-3 py-2 text-sm font-mono bg-[#1e1e1e] border border-[#333] rounded-lg focus:border-[#3b82f6] focus:outline-none"
              placeholder="Sender address (e.g., 192.168.1.5:12345)"
            />
            <input
              type="text"
              value={saveDir()}
              onInput={(e) => setSaveDir(e.currentTarget.value)}
              class="w-full px-3 py-2 text-sm bg-[#1e1e1e] border border-[#333] rounded-lg focus:border-[#3b82f6] focus:outline-none"
              placeholder="Save to (default: /tmp/relay-received)"
            />
          </div>

          <div class="text-center">
            <button
              class="text-sm text-[#a0a0a0] hover:text-white transition-colors"
              onClick={resetTransfer}
            >
              Back
            </button>
          </div>
        </div>
      </Show>

      <Show when={transfer.phase === "connecting"}>
        <div class="text-center space-y-4">
          <div class="w-12 h-12 mx-auto border-2 border-[#3b82f6] border-t-transparent rounded-full animate-spin" />
          <p class="text-[#a0a0a0]">Connecting to sender...</p>
          <button
            class="text-sm text-[#a0a0a0] hover:text-white transition-colors"
            onClick={resetTransfer}
          >
            Cancel
          </button>
        </div>
      </Show>

      <Show when={transfer.phase === "offer"}>
        <div class="space-y-6">
          <h2 class="text-xl font-semibold text-center">Incoming Files</h2>
          <div class="bg-[#141414] border border-[#333] rounded-xl p-4">
            <FileList
              files={transfer.offerFiles.map((f) => ({
                name: f.name,
                size: f.size,
              }))}
            />
            <div class="mt-3 pt-3 border-t border-[#333] text-sm text-[#a0a0a0] text-right">
              {transfer.offerFiles.length} file(s),{" "}
              {formatBytes(
                transfer.offerFiles.reduce((sum, f) => sum + f.size, 0)
              )}
            </div>
          </div>
          <div class="flex gap-3 justify-center">
            <button
              class="px-6 py-3 bg-[#3b82f6] hover:bg-[#2563eb] rounded-lg font-semibold transition-colors"
              onClick={() => handleAccept(true)}
            >
              Accept
            </button>
            <button
              class="px-6 py-3 bg-[#1e1e1e] hover:bg-[#2a2a2a] border border-[#333] rounded-lg transition-colors"
              onClick={() => handleAccept(false)}
            >
              Decline
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
}
