/** Appearance preferences (theme/variant/accent/typeface/radius), persisted locally. */
export interface Tweaks {
  accent: string;
  fontPair: "grotesk" | "plex" | "sora";
  radius: number;
  theme: "dark" | "light";
  variant: "console" | "refined";
}

const KEY = "naravault.tweaks.v1";

const DEFAULTS: Tweaks = {
  accent: "#dfe0e3",
  fontPair: "grotesk",
  radius: 10,
  theme: "dark",
  variant: "console",
};

const FONT_PAIRS: Record<Tweaks["fontPair"], { ui: string; mono: string }> = {
  grotesk: { ui: "'Space Grotesk', sans-serif", mono: "'JetBrains Mono', monospace" },
  plex: { ui: "'IBM Plex Sans', sans-serif", mono: "'IBM Plex Mono', monospace" },
  sora: { ui: "'Sora', sans-serif", mono: "'Space Mono', monospace" },
};

function load(): Tweaks {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...DEFAULTS, ...JSON.parse(raw) };
  } catch {
    /* ignore */
  }
  return { ...DEFAULTS };
}

class TweaksStore {
  t = $state<Tweaks>(load());

  set<K extends keyof Tweaks>(key: K, value: Tweaks[K]) {
    this.t = { ...this.t, [key]: value };
    try {
      localStorage.setItem(KEY, JSON.stringify(this.t));
    } catch {
      /* ignore */
    }
  }

  /** Apply the current tweaks to the document root. Call from an $effect. */
  apply() {
    const r = document.documentElement;
    r.setAttribute("data-theme", this.t.theme);
    r.setAttribute("data-variant", this.t.variant);
    r.style.setProperty("--accent", this.t.accent);
    r.style.setProperty("--radius", `${this.t.radius}px`);
    const fp = FONT_PAIRS[this.t.fontPair] ?? FONT_PAIRS.grotesk;
    r.style.setProperty("--font-ui", fp.ui);
    r.style.setProperty("--font-mono", fp.mono);
  }
}

export const tweaks = new TweaksStore();
