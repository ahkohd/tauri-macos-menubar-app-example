import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { Tabs } from "./components/Tabs";
import { ProjectList } from "./components/ProjectList";
import { LogsViewer } from "./components/LogsViewer";
import { Settings } from "./components/Settings";
import * as api from "./api";
import type { Tab, FileChange } from "./types";

import "./App.css";

function App() {
  const [activeTab, setActiveTab] = useState<Tab>("projects");
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const initialize = async () => {
      invoke("init");

      // Check if we have an access token, if not show settings
      const hasToken = await api.hasAccessToken();
      if (!hasToken) {
        setActiveTab("settings");
      }
      setIsLoading(false);
    };

    initialize();

    // Listen for file changes to potentially auto-switch to logs
    const unlisten = listen<FileChange>("file_change", (event) => {
      console.log("File changed:", event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const renderContent = () => {
    switch (activeTab) {
      case "projects":
        return <ProjectList />;
      case "logs":
        return <LogsViewer />;
      case "settings":
        return <Settings />;
    }
  };

  if (isLoading) {
    return (
      <div className="app">
        <div className="loading-screen">Loading...</div>
      </div>
    );
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>Supawatch</h1>
      </header>

      <Tabs activeTab={activeTab} onTabChange={setActiveTab} />

      <main className="app-content">{renderContent()}</main>
    </div>
  );
}

export default App;
