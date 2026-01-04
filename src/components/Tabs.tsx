import type { Tab } from "../types";
import "./Tabs.css";

interface TabsProps {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
}

export function Tabs({ activeTab, onTabChange }: TabsProps) {
  return (
    <div className="tabs">
      <button
        className={`tab ${activeTab === "projects" ? "active" : ""}`}
        onClick={() => onTabChange("projects")}
      >
        Projects
      </button>
      <button
        className={`tab ${activeTab === "logs" ? "active" : ""}`}
        onClick={() => onTabChange("logs")}
      >
        Logs
      </button>
      <button
        className={`tab ${activeTab === "settings" ? "active" : ""}`}
        onClick={() => onTabChange("settings")}
      >
        Settings
      </button>
    </div>
  );
}
