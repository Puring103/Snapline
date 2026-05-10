import ReactDOM from "react-dom/client";
import { App } from "./App";
import { startupLog } from "./platform/startupLog";
import "./styles.css";

startupLog("js_entry");
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<App />);
startupLog("react_render_scheduled");
