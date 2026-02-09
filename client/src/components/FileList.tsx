import { For } from "solid-js";
import { formatBytes } from "../lib/format";

interface FileItem {
  name: string;
  size: number;
  status?: "pending" | "transferring" | "completed" | "failed";
}

interface Props {
  files: FileItem[];
  compact?: boolean;
}

export default function FileList(props: Props) {
  return (
    <div class="space-y-1">
      <For each={props.files}>
        {(file) => (
          <div
            class={`flex items-center justify-between px-3 py-2 rounded-lg ${
              props.compact ? "bg-transparent" : "bg-[#141414]"
            }`}
          >
            <div class="flex items-center gap-2 min-w-0">
              <span class="text-[#a0a0a0] text-sm flex-shrink-0">
                {file.status === "completed"
                  ? "\u2713"
                  : file.status === "transferring"
                    ? "\u25B6"
                    : file.status === "failed"
                      ? "\u2717"
                      : "\u2022"}
              </span>
              <span class="truncate text-sm">{file.name}</span>
            </div>
            <span class="text-[#a0a0a0] text-xs flex-shrink-0 ml-3">
              {formatBytes(file.size)}
            </span>
          </div>
        )}
      </For>
    </div>
  );
}
