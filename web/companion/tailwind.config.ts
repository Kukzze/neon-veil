import type { Config } from "tailwindcss";

// Palette shared with the in-game HUD (crates/game_ui/src/hud.rs).
const config: Config = {
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        void: "#030712",
        panel: "#0a111c",
        line: "#18222f",
        mist: "#8b98a9",
        cyan: { neon: "#00E5FF", deep: "#00a8bd" },
        emerald: { neon: "#2ECC71" },
        gold: { neon: "#FFAA00" },
        rarity: {
          common: "#b8c0cc",
          uncommon: "#2ECC71",
          rare: "#3498db",
          epic: "#ab63e6",
          legendary: "#FFAA00",
          exotic: "#00E5FF",
        },
      },
      fontFamily: {
        display: ["var(--font-display)", "system-ui", "sans-serif"],
        mono: ["var(--font-mono)", "ui-monospace", "monospace"],
      },
      letterSpacing: {
        hud: "0.08em",
      },
    },
  },
  plugins: [],
};

export default config;
