import { Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { transfer, setTransfer } from "../stores/transfer";
import { startSend } from "../lib/tauri-bridge";
import { resetTransfer } from "../stores/transfer";
import CodeDisplay from "./CodeDisplay";
import FileList from "./FileList";

export default function SendView() {
  async function selectFiles() {
    const selected = await open({
      multiple: true,
      directory: false,
    });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      setTransfer("selectedFiles", paths);
    }
  }

  async function handleSend() {
    if (transfer.selectedFiles.length === 0) return;
    try {
      const result = await startSend(transfer.selectedFiles);
      setTransfer("code", result.code);
      setTransfer("sessionId", result.session_id);
      setTransfer("senderPort", result.port);
      setTransfer("phase", "waiting");
    } catch (e) {
      setTransfer("phase", "error");
      setTransfer("error", String(e));
    }
  }

  return (
    <div class="w-full max-w-md space-y-6">
      <Show when={transfer.phase === "selecting"}>
        <div class="text-center space-y-6">
          <h2 class="text-2xl font-semibold">Send Files</h2>

          <button
            class="w-full py-12 border-2 border-dashed border-[#333] hover:border-[#3b82f6] rounded-xl text-[#a0a0a0] hover:text-white transition-colors"
            onClick={selectFiles}
          >
            <div class="space-y-2">
              <p class="text-lg">Click to select files</p>
              <p class="text-sm">or drag and drop (coming soon)</p>
            </div>
          </button>

          <Show when={transfer.selectedFiles.length > 0}>
            <FileList
              files={transfer.selectedFiles.map((path) => ({
                name: path.split("/").pop() || path,
                size: 0,
              }))}
            />
            <button
              class="w-full py-3 bg-[#3b82f6] hover:bg-[#2563eb] rounded-lg font-semibold transition-colors"
              onClick={handleSend}
            >
              Start Sending
            </button>
          </Show>

          <button
            class="text-sm text-[#a0a0a0] hover:text-white transition-colors"
            onClick={resetTransfer}
          >
            Back
          </button>
        </div>
      </Show>

      <Show when={transfer.phase === "waiting"}>
        <div class="text-center space-y-6">
          <CodeDisplay code={transfer.code} />
          <div class="flex items-center justify-center gap-2 text-[#a0a0a0]">
            <div class="w-2 h-2 bg-[#3b82f6] rounded-full animate-pulse" />
            <p>Waiting for receiver...</p>
          </div>
          <Show when={transfer.senderPort > 0}>
            <p class="text-xs text-[#555]">
              Listening on port {transfer.senderPort}
            </p>
          </Show>
          <button
            class="text-sm text-[#a0a0a0] hover:text-white transition-colors"
            onClick={resetTransfer}
          >
            Cancel
          </button>
        </div>
      </Show>
    </div>
  );
}
