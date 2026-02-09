import { createStore } from "solid-js/store";

export interface SettingsStore {
  signalServerUrl: string;
  defaultSaveDir: string;
  theme: "dark" | "light";
}

const stored = {
  signalServerUrl:
    localStorage.getItem("relay:signalServerUrl") || "wss://relay-signal.fly.dev",
  defaultSaveDir: localStorage.getItem("relay:defaultSaveDir") || "",
  theme: (localStorage.getItem("relay:theme") as "dark" | "light") || "dark",
};

export const [settings, setSettings] = createStore<SettingsStore>(stored);

export function updateSetting<K extends keyof SettingsStore>(
  key: K,
  value: SettingsStore[K]
) {
  setSettings(key, value);
  localStorage.setItem(`relay:${key}`, String(value));
}
