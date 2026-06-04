import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./fonts.css";
import "./app.css";
import App from "./App.svelte";
import Launcher from "./screens/Launcher.svelte";

// The same bundle serves both windows; the Tauri window label decides which
// root component to mount. Fall back to the main app outside Tauri.
let label = "main";
try {
  label = getCurrentWindow().label;
} catch {
  label = "main";
}

const target = document.getElementById("root")!;
if (label === "launcher") {
  document.documentElement.classList.add("launcher-window");
  document.body.classList.add("launcher-window");
}
const app =
  label === "launcher"
    ? mount(Launcher, { target })
    : mount(App, { target });

export default app;
