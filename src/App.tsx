import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { Tabs } from "./components/Tabs";
import { ProjectList } from "./components/ProjectList";
import { LogsViewer } from "./components/LogsViewer";
import type { Tab, FileChange } from "./types";

import "./App.css";

function App() {
  const [activeTab, setActiveTab] = useState<Tab>("projects");

  useEffect(() => {
    invoke("init");

    // Listen for file changes to potentially auto-switch to logs
    const unlisten = listen<FileChange>("file_change", (event) => {
      console.log("File changed:", event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <div className="app">
      <header className="app-header">
        <h1>Supawatch</h1>
      </header>

      <Tabs activeTab={activeTab} onTabChange={setActiveTab} />

      <main className="app-content">
        {activeTab === "projects" ? <ProjectList /> : <LogsViewer />}
      </main>
    </div>
  );
}

export default App;
