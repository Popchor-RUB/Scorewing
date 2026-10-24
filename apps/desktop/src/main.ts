import "./style.css";
import { mountApp } from "./app.tsx";

const dispose = mountApp(document.getElementById("app")!);
if (import.meta.hot) import.meta.hot.dispose(dispose);
