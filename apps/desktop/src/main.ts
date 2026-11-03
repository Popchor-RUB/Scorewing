import "./style.css";

const { mountApp } = await (new URLSearchParams(window.location.search).has("update-prompt")
  ? import("./update-prompt.tsx") : import("./app.tsx"));

const dispose = mountApp(document.getElementById("app")!);
if (import.meta.hot) import.meta.hot.dispose(dispose);
