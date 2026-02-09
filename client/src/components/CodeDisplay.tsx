import { createSignal } from "solid-js";

interface Props {
  code: string;
}

export default function CodeDisplay(props: Props) {
  const [copied, setCopied] = createSignal(false);

  async function copyCode() {
    await navigator.clipboard.writeText(props.code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div class="space-y-3 text-center">
      <p class="text-sm text-[#a0a0a0] uppercase tracking-wider">
        Transfer Code
      </p>
      <div
        class="inline-flex items-center gap-3 px-6 py-4 bg-[#1e1e1e] border border-[#333] rounded-xl cursor-pointer hover:border-[#3b82f6] transition-colors"
        onClick={copyCode}
      >
        <span class="text-3xl font-mono font-bold tracking-widest">
          {props.code}
        </span>
        <button class="text-[#a0a0a0] hover:text-white text-sm transition-colors">
          {copied() ? "Copied!" : "Copy"}
        </button>
      </div>
      <p class="text-sm text-[#a0a0a0]">
        Share this code with the receiver
      </p>
    </div>
  );
}
