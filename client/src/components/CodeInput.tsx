import { createSignal, For } from "solid-js";
import { WORDLIST } from "../lib/wordlist";

interface Props {
  onSubmit: (code: string) => void;
  disabled?: boolean;
}

export default function CodeInput(props: Props) {
  const [digit, setDigit] = createSignal("");
  const [word1, setWord1] = createSignal("");
  const [word2, setWord2] = createSignal("");
  const [suggestions1, setSuggestions1] = createSignal<string[]>([]);
  const [suggestions2, setSuggestions2] = createSignal<string[]>([]);

  let word1Ref: HTMLInputElement | undefined;
  let word2Ref: HTMLInputElement | undefined;

  function getSuggestions(input: string): string[] {
    if (input.length < 1) return [];
    const lower = input.toLowerCase();
    return WORDLIST.filter((w) => w.startsWith(lower)).slice(0, 5);
  }

  function handleDigitInput(value: string) {
    const clean = value.replace(/\D/g, "").slice(0, 1);
    setDigit(clean);
    if (clean.length === 1) word1Ref?.focus();
  }

  function handleSubmit() {
    const code = `${digit()}-${word1().toLowerCase()}-${word2().toLowerCase()}`;
    props.onSubmit(code);
  }

  function isValid(): boolean {
    return (
      digit().length === 1 &&
      WORDLIST.includes(word1().toLowerCase()) &&
      WORDLIST.includes(word2().toLowerCase())
    );
  }

  return (
    <div class="space-y-4">
      <p class="text-sm text-[#a0a0a0] uppercase tracking-wider text-center">
        Enter Transfer Code
      </p>
      <div class="flex items-center gap-2 justify-center">
        <input
          type="text"
          inputMode="numeric"
          maxLength={1}
          value={digit()}
          onInput={(e) => handleDigitInput(e.currentTarget.value)}
          class="w-14 h-14 text-center text-2xl font-mono bg-[#1e1e1e] border border-[#333] rounded-lg focus:border-[#3b82f6] focus:outline-none transition-colors"
          placeholder="0"
          disabled={props.disabled}
        />
        <span class="text-2xl text-[#555]">-</span>
        <div class="relative">
          <input
            ref={word1Ref}
            type="text"
            value={word1()}
            onInput={(e) => {
              setWord1(e.currentTarget.value);
              setSuggestions1(getSuggestions(e.currentTarget.value));
            }}
            onFocus={() => setSuggestions1(getSuggestions(word1()))}
            onBlur={() => setTimeout(() => setSuggestions1([]), 200)}
            class="w-36 h-14 px-3 text-lg font-mono bg-[#1e1e1e] border border-[#333] rounded-lg focus:border-[#3b82f6] focus:outline-none transition-colors"
            placeholder="word"
            disabled={props.disabled}
          />
          {suggestions1().length > 0 && (
            <div class="absolute top-full left-0 w-full mt-1 bg-[#1e1e1e] border border-[#333] rounded-lg overflow-hidden z-10">
              <For each={suggestions1()}>
                {(word) => (
                  <button
                    class="w-full px-3 py-2 text-left text-sm font-mono hover:bg-[#2a2a2a] transition-colors"
                    onMouseDown={() => {
                      setWord1(word);
                      setSuggestions1([]);
                      word2Ref?.focus();
                    }}
                  >
                    {word}
                  </button>
                )}
              </For>
            </div>
          )}
        </div>
        <span class="text-2xl text-[#555]">-</span>
        <div class="relative">
          <input
            ref={word2Ref}
            type="text"
            value={word2()}
            onInput={(e) => {
              setWord2(e.currentTarget.value);
              setSuggestions2(getSuggestions(e.currentTarget.value));
            }}
            onFocus={() => setSuggestions2(getSuggestions(word2()))}
            onBlur={() => setTimeout(() => setSuggestions2([]), 200)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && isValid()) handleSubmit();
            }}
            class="w-36 h-14 px-3 text-lg font-mono bg-[#1e1e1e] border border-[#333] rounded-lg focus:border-[#3b82f6] focus:outline-none transition-colors"
            placeholder="word"
            disabled={props.disabled}
          />
          {suggestions2().length > 0 && (
            <div class="absolute top-full left-0 w-full mt-1 bg-[#1e1e1e] border border-[#333] rounded-lg overflow-hidden z-10">
              <For each={suggestions2()}>
                {(word) => (
                  <button
                    class="w-full px-3 py-2 text-left text-sm font-mono hover:bg-[#2a2a2a] transition-colors"
                    onMouseDown={() => {
                      setWord2(word);
                      setSuggestions2([]);
                    }}
                  >
                    {word}
                  </button>
                )}
              </For>
            </div>
          )}
        </div>
      </div>
      <div class="text-center">
        <button
          class="px-6 py-3 bg-[#3b82f6] hover:bg-[#2563eb] disabled:opacity-40 disabled:cursor-not-allowed rounded-lg font-semibold transition-colors"
          onClick={handleSubmit}
          disabled={!isValid() || props.disabled}
        >
          Connect
        </button>
      </div>
    </div>
  );
}
