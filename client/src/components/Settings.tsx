import { settings, updateSetting } from "../stores/settings";

interface Props {
  onClose: () => void;
}

export default function Settings(props: Props) {
  return (
    <div class="w-full max-w-md space-y-6">
      <div class="flex items-center justify-between">
        <h2 class="text-xl font-semibold">Settings</h2>
        <button
          class="text-[#a0a0a0] hover:text-white transition-colors"
          onClick={props.onClose}
        >
          Close
        </button>
      </div>

      <div class="space-y-4">
        <div class="space-y-2">
          <label class="text-sm text-[#a0a0a0]">Signaling Server URL</label>
          <input
            type="text"
            value={settings.signalServerUrl}
            onInput={(e) =>
              updateSetting("signalServerUrl", e.currentTarget.value)
            }
            class="w-full px-3 py-2 text-sm bg-[#1e1e1e] border border-[#333] rounded-lg focus:border-[#3b82f6] focus:outline-none"
          />
        </div>

        <div class="space-y-2">
          <label class="text-sm text-[#a0a0a0]">Default Save Directory</label>
          <input
            type="text"
            value={settings.defaultSaveDir}
            onInput={(e) =>
              updateSetting("defaultSaveDir", e.currentTarget.value)
            }
            class="w-full px-3 py-2 text-sm bg-[#1e1e1e] border border-[#333] rounded-lg focus:border-[#3b82f6] focus:outline-none"
            placeholder="System Downloads folder"
          />
        </div>

        <div class="space-y-2">
          <label class="text-sm text-[#a0a0a0]">Theme</label>
          <select
            value={settings.theme}
            onChange={(e) =>
              updateSetting(
                "theme",
                e.currentTarget.value as "dark" | "light"
              )
            }
            class="w-full px-3 py-2 text-sm bg-[#1e1e1e] border border-[#333] rounded-lg focus:border-[#3b82f6] focus:outline-none"
          >
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </div>
      </div>
    </div>
  );
}
