import { Show } from "solid-js";
import { transfer } from "../stores/transfer";

export default function ConnectionStatus() {
  const isActive = () =>
    transfer.phase === "transferring" ||
    transfer.phase === "connecting" ||
    transfer.phase === "waiting";

  const indicatorClass = () => {
    if (transfer.connectionType === "direct") return "bg-[#22c55e]";
    if (transfer.connectionType === "relay") return "bg-[#eab308]";
    return "bg-[#6b7280]";
  };

  const connectionLabel = () => {
    if (transfer.connectionType === "direct") return "Direct P2P";
    if (transfer.connectionType === "relay") return "Relay";
    return "Negotiating";
  };

  return (
    <Show when={isActive()}>
      <div class="flex items-center justify-between px-4 py-2 bg-[#0f0f0f] border-t border-[#1e1e1e] text-xs text-[#a0a0a0]">
        <div class="flex items-center gap-4">
          <div class="flex items-center gap-1.5">
            <div class={`w-2 h-2 rounded-full ${indicatorClass()}`} />
            <span>{connectionLabel()}</span>
          </div>
          <div class="flex items-center gap-1.5">
            <span>AES-256-GCM</span>
          </div>
        </div>
      </div>
    </Show>
  );
}
