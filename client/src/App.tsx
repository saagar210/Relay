import { onMount, onCleanup, Switch, Match } from "solid-js";
import { transfer, setTransfer, resetTransfer } from "./stores/transfer";
import { onTransferProgress, type ProgressEvent } from "./lib/tauri-bridge";
import SendView from "./components/SendView";
import ReceiveView from "./components/ReceiveView";
import TransferProgress from "./components/TransferProgress";
import CompletionView from "./components/CompletionView";
import ConnectionStatus from "./components/ConnectionStatus";
import "./styles/app.css";

export default function App() {
  let unlisten: (() => void) | undefined;

  onMount(async () => {
    unlisten = await onTransferProgress(handleProgressEvent);
  });

  onCleanup(() => {
    unlisten?.();
  });

  function handleProgressEvent(event: ProgressEvent) {
    switch (event.type) {
      case "transferProgress":
        setTransfer("phase", "transferring");
        setTransfer("progress", {
          bytesTransferred: event.bytes_transferred,
          bytesTotal: event.bytes_total,
          speedBps: event.speed_bps,
          etaSeconds: event.eta_seconds,
          currentFile: event.current_file,
          percent: event.percent,
          completedFiles: transfer.progress.completedFiles,
        });
        break;
      case "fileCompleted":
        setTransfer("progress", "completedFiles", [
          ...transfer.progress.completedFiles,
          event.name,
        ]);
        break;
      case "transferComplete":
        setTransfer("phase", "completed");
        setTransfer("summary", {
          fileCount: event.file_count,
          totalBytes: event.total_bytes,
          durationSeconds: event.duration_seconds,
          averageSpeed: event.average_speed,
        });
        break;
      case "fileOffer":
        setTransfer("phase", "offer");
        setTransfer("sessionId", event.session_id);
        setTransfer("offerFiles", event.files);
        break;
      case "error":
        setTransfer("phase", "error");
        setTransfer("error", event.message);
        break;
      case "stateChanged":
        if (event.state === "connecting") {
          setTransfer("phase", "connecting");
        }
        break;
    }
  }

  return (
    <div class="flex flex-col h-screen bg-[#0a0a0a] text-[#f5f5f5]">
      <main class="flex-1 flex items-center justify-center p-6 overflow-auto">
        <Switch>
          <Match when={transfer.phase === "idle"}>
            <HomeScreen />
          </Match>
          <Match
            when={
              transfer.phase === "selecting" || transfer.phase === "waiting"
            }
          >
            <SendView />
          </Match>
          <Match
            when={
              transfer.phase === "entering-code" ||
              transfer.phase === "connecting" ||
              transfer.phase === "offer"
            }
          >
            <ReceiveView />
          </Match>
          <Match when={transfer.phase === "transferring"}>
            <TransferProgress />
          </Match>
          <Match when={transfer.phase === "completed"}>
            <CompletionView />
          </Match>
          <Match when={transfer.phase === "error"}>
            <ErrorScreen />
          </Match>
        </Switch>
      </main>
      <ConnectionStatus />
    </div>
  );
}

function HomeScreen() {
  return (
    <div class="text-center space-y-8 max-w-md">
      <div class="space-y-2">
        <h1 class="text-4xl font-bold tracking-tight">Relay</h1>
        <p class="text-[#a0a0a0] text-lg">
          Share files. No cloud. No accounts.
        </p>
      </div>
      <div class="flex gap-4 justify-center">
        <button
          class="px-8 py-4 bg-[#3b82f6] hover:bg-[#2563eb] rounded-xl text-lg font-semibold transition-colors"
          onClick={() => {
            setTransfer("phase", "selecting");
            setTransfer("role", "sender");
          }}
        >
          Send
        </button>
        <button
          class="px-8 py-4 bg-[#1e1e1e] hover:bg-[#2a2a2a] border border-[#333] rounded-xl text-lg font-semibold transition-colors"
          onClick={() => {
            setTransfer("phase", "entering-code");
            setTransfer("role", "receiver");
          }}
        >
          Receive
        </button>
      </div>
    </div>
  );
}

function ErrorScreen() {
  return (
    <div class="text-center space-y-6 max-w-md">
      <div class="w-16 h-16 mx-auto rounded-full bg-red-500/20 flex items-center justify-center">
        <span class="text-red-400 text-2xl">!</span>
      </div>
      <div class="space-y-2">
        <h2 class="text-xl font-semibold">Transfer Failed</h2>
        <p class="text-[#a0a0a0]">{transfer.error}</p>
      </div>
      <button
        class="px-6 py-3 bg-[#1e1e1e] hover:bg-[#2a2a2a] border border-[#333] rounded-lg transition-colors"
        onClick={resetTransfer}
      >
        Try Again
      </button>
    </div>
  );
}
