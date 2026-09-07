import "./styles.css";
import App from "./App.svelte";
import { mount } from "svelte";

// The native window uses an overlay title bar, so macOS draws its traffic
// lights on top of our tab strip. Flag that case so the inset that clears
// them applies only there, and the browser demo keeps a flush strip.
if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
  document.documentElement.classList.add("native-shell");
}

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
