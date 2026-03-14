import { createSignal, onMount, onCleanup, Switch, Match, Show } from "solid-js";
import { transfer, setTransfer, resetTransfer, addSpeedDataPoint } from "./stores/transfer";
import { settings, applyTheme } from "./stores/settings";
import { onTransferProgress, cancelTransfer, type ProgressEvent } from "./lib/tauri-bridge";
import { open } from "@tauri-apps/plugin-dialog";
import SendView from "./components/SendView";
import ReceiveView from "./components/ReceiveView";
import TransferProgress from "./components/TransferProgress";
import CompletionView from "./components/CompletionView";
import ConnectionStatus from "./components/ConnectionStatus";
import Settings from "./components/Settings";
import "./styles/app.css";

export default function App() {
  let unlisten: (() => void) | undefined;
  const [showSettings, setShowSettings] = createSignal(false);
  const [isDragActive, setIsDragActive] = createSignal(false);

  onMount(async () => {
    unlisten = await onTransferProgress(handleProgressEvent);

    // Add drag-and-drop handlers
    const handleDragOver = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.dataTransfer) {
        e.dataTransfer.dropEffect = "copy";
      }
      setIsDragActive(true);
    };

    const handleDragLeave = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragActive(false);
    };

    const handleDrop = async (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragActive(false);

      // Only handle drops when in idle or selecting phase
      if (transfer.phase !== "idle" && transfer.phase !== "selecting") {
        return;
      }

      if (e.dataTransfer?.files && e.dataTransfer.files.length > 0) {
        const paths: string[] = [];
        for (let i = 0; i < e.dataTransfer.files.length; i++) {
          const file = e.dataTransfer.files[i];
          // @ts-ignore - Tauri adds path property to File objects
          if (file.path) {
            // @ts-ignore
            paths.push(file.path);
          }
        }

        if (paths.length > 0) {
          setTransfer("phase", "selecting");
          setTransfer("role", "sender");
          setTransfer("selectedFiles", paths);
        }
      }
    };

    document.addEventListener("dragover", handleDragOver);
    document.addEventListener("dragleave", handleDragLeave);
    document.addEventListener("drop", handleDrop);

    // Listen for system theme changes
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handleThemeChange = () => {
      if (settings.theme === "system") {
        applyTheme("system");
      }
    };
    mediaQuery.addEventListener("change", handleThemeChange);

    // Add keyboard shortcuts
    const handleKeyDown = async (e: KeyboardEvent) => {
      const isMac = navigator.platform.toUpperCase().indexOf("MAC") >= 0;
      const metaKey = isMac ? e.metaKey : e.ctrlKey;

      // Don't trigger shortcuts when user is typing in an input/textarea
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") {
        return;
      }

      // Cmd+O / Ctrl+O: Open file picker
      if (metaKey && e.key === "o") {
        e.preventDefault();
        if (transfer.phase === "idle") {
          const selected = await open({ multiple: true, directory: false });
          if (selected) {
            const paths = Array.isArray(selected) ? selected : [selected];
            setTransfer("phase", "selecting");
            setTransfer("role", "sender");
            setTransfer("selectedFiles", paths);
          }
        }
      }

      // Cmd+V / Ctrl+V: Paste transfer code
      if (metaKey && e.key === "v") {
        if (transfer.phase === "idle") {
          e.preventDefault();
          try {
            const clipboardText = await navigator.clipboard.readText();
            if (clipboardText) {
              // Check if it matches transfer code format (digit-word-word)
              const codePattern = /^\d{1,2}-[a-z]+-[a-z]+$/i;
              if (codePattern.test(clipboardText.trim())) {
                setTransfer("phase", "entering-code");
                setTransfer("role", "receiver");
              }
            }
          } catch (err) {
            // Clipboard permission denied or not available
            console.warn("Clipboard read failed:", err);
          }
        }
      }

      // Escape: Cancel transfer or go back
      if (e.key === "Escape") {
        if (transfer.phase === "transferring" || transfer.phase === "waiting" || transfer.phase === "connecting") {
          e.preventDefault();
          if (transfer.sessionId) {
            try {
              await cancelTransfer(transfer.sessionId);
            } catch (err) {
              console.warn("Cancel transfer failed:", err);
            }
          }
          resetTransfer();
        } else if (transfer.phase !== "idle") {
          e.preventDefault();
          resetTransfer();
        }
      }

      // Cmd+, / Ctrl+,: Open settings
      if (metaKey && e.key === ",") {
        e.preventDefault();
        setShowSettings(!showSettings());
      }
    };
    document.addEventListener("keydown", handleKeyDown);

    return () => {
      document.removeEventListener("dragover", handleDragOver);
      document.removeEventListener("dragleave", handleDragLeave);
      document.removeEventListener("drop", handleDrop);
      mediaQuery.removeEventListener("change", handleThemeChange);
      document.removeEventListener("keydown", handleKeyDown);
    };
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
          speedHistory: transfer.progress.speedHistory,
        });
        // Add speed data point for graph
        addSpeedDataPoint(event.speed_bps);
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
        // Keep an existing session id if an older backend omits it in FileOffer.
        if (event.session_id && event.session_id.trim().length > 0) {
          setTransfer("sessionId", event.session_id);
        }
        setTransfer("offerFiles", event.files);
        break;
      case "error":
        setTransfer("phase", "error");
        setTransfer("error", event.message);
        break;
      case "stateChanged":
        if (event.state === "connecting") {
          setTransfer("phase", "connecting");
          setTransfer("connectionType", "negotiating");
        }
        break;
      case "connectionTypeChanged":
        setTransfer(
          "connectionType",
          event.connection_type === "relay" ? "relay" : "direct"
        );
        break;
    }
  }

  return (
    <div class="flex flex-col h-screen bg-[#0a0a0a] text-[#f5f5f5]">
      <main class="flex-1 flex items-center justify-center p-6 overflow-auto">
        <Switch>
          <Match when={showSettings()}>
            <Settings onClose={() => setShowSettings(false)} />
          </Match>
          <Match when={transfer.phase === "idle"}>
            <HomeScreen onOpenSettings={() => setShowSettings(true)} />
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

      {/* Drag-and-drop overlay */}
      <Show when={isDragActive()}>
        <div class="drag-overlay">
          <div class="drag-overlay-content">
            <svg class="w-16 h-16 text-white mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12"></path>
            </svg>
            <p class="text-2xl font-bold text-white">Drop files to send</p>
            <p class="text-sm text-gray-400 mt-2">Files and folders are supported</p>
          </div>
        </div>
      </Show>
    </div>
  );
}

function HomeScreen(props: { onOpenSettings: () => void }) {
  return (
    <div class="text-center space-y-8 max-w-md">
      <div class="flex justify-end">
        <button
          class="text-sm text-[#a0a0a0] hover:text-white transition-colors"
          onClick={props.onOpenSettings}
        >
          Settings
        </button>
      </div>
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
