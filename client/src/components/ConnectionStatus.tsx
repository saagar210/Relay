import { Show } from "solid-js";
import { transfer } from "../stores/transfer";

export default function ConnectionStatus() {
  const isActive = () =>
    transfer.phase === "transferring" ||
    transfer.phase === "connecting" ||
    transfer.phase === "waiting";

  return (
    <Show when={isActive()}>
      <div class="flex items-center justify-between px-4 py-2 bg-[#0f0f0f] border-t border-[#1e1e1e] text-xs text-[#a0a0a0]">
        <div class="flex items-center gap-4">
          <div class="flex items-center gap-1.5">
            <div
              class={`w-2 h-2 rounded-full ${
                transfer.connectionType === "direct"
                  ? "bg-[#22c55e]"
                  : "bg-[#eab308]"
              }`}
            />
            <span>
              {transfer.connectionType === "direct"
                ? "Direct P2P"
                : "Relayed"}
            </span>
          </div>
          <div class="flex items-center gap-1.5">
            <span>AES-256-GCM</span>
          </div>
        </div>
      </div>
    </Show>
  );
}
