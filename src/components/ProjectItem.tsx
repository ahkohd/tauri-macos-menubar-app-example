import { useState } from "react";
import * as api from "../api";
import type { Project } from "../types";
import "./ProjectItem.css";

interface ProjectItemProps {
  project: Project;
  onUpdate: () => void;
  onDelete: () => void;
}

export function ProjectItem({ project, onUpdate, onDelete }: ProjectItemProps) {
  const [isWatching, setIsWatching] = useState(project.is_watching);
  const [isLoading, setIsLoading] = useState(false);
  const [showActions, setShowActions] = useState(false);

  const toggleWatch = async () => {
    setIsLoading(true);
    try {
      if (isWatching) {
        await api.stopWatching(project.id);
      } else {
        await api.startWatching(project.id);
      }
      setIsWatching(!isWatching);
      onUpdate();
    } catch (err) {
      console.error("Failed to toggle watch:", err);
    } finally {
      setIsLoading(false);
    }
  };

  const handlePull = async () => {
    if (
      !confirm(
        `Overwrite local changes for "${project.name}"? This cannot be undone.`
      )
    )
      return;
    setIsLoading(true);
    try {
      await api.pullProject(project.id);
      alert("Project pulled successfully");
    } catch (err) {
      console.error("Failed to pull project:", err);
      alert("Failed to pull project: " + String(err));
    } finally {
      setIsLoading(false);
    }
  };

  const handleDelete = async () => {
    if (!confirm(`Delete project "${project.name}"?`)) return;
    try {
      await api.deleteProject(project.id);
      onDelete();
    } catch (err) {
      console.error("Failed to delete project:", err);
    }
  };

  return (
    <div
      className={`project-item ${isWatching ? "watching" : ""}`}
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
    >
      <div className="project-info">
        <div className="project-header">
          <span className={`status-dot ${isWatching ? "active" : ""}`} />
          <span className="project-name">{project.name}</span>
        </div>
        <div className="project-path" title={project.local_path}>
          {project.local_path}
        </div>
        {project.supabase_project_ref && (
          <div className="project-ref">{project.supabase_project_ref}</div>
        )}
      </div>

      <div className={`project-actions ${showActions ? "visible" : ""}`}>
        <button
          className={`action-btn ${isWatching ? "stop" : "start"}`}
          onClick={toggleWatch}
          disabled={isLoading}
          title={isWatching ? "Stop watching" : "Start watching"}
        >
          {isLoading ? "..." : isWatching ? "Stop" : "Watch"}
        </button>
        <button
          className="action-btn pull"
          onClick={handlePull}
          disabled={isLoading}
          title="Pull from remote (overwrites local)"
        >
          Pull
        </button>
        <button
          className="action-btn delete"
          onClick={handleDelete}
          title="Delete project"
        >
          Delete
        </button>
      </div>
    </div>
  );
}
